use alloy::primitives::{Address, Bytes, FixedBytes, U256};
use alloy::sol_types::SolValue;
use futures::{stream::FuturesUnordered, StreamExt};
use once_cell::sync::{Lazy, OnceCell};
use raindex_bindings::IRaindexV7::{OrderV4, SignedContextV2};
use raindex_subgraph_client::types::common::SgOrder;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    future::Future,
    num::NonZeroUsize,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::sync::Semaphore;
use url::Url;

/// Default maximum number of concurrent oracle HTTP requests per caller and
/// oracle origin.
///
/// Eight allows meaningful overlap for quote pages while keeping pressure on
/// each oracle service conservative. Native clients share each origin's limit
/// across the process; WASM clients share it within one module instance.
pub(crate) const ORACLE_REQUEST_CONCURRENCY_LIMIT: usize = 8;
const ORACLE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

static ORACLE_HTTP_CLIENT: OnceCell<Client> = OnceCell::new();
type OracleRequestSemaphores = Mutex<HashMap<String, Weak<Semaphore>>>;
static ORACLE_REQUEST_SEMAPHORES: Lazy<Arc<OracleRequestSemaphores>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Validate that an oracle URL is safe to POST to.
/// Only http and https schemes are allowed to prevent SSRF.
fn validate_oracle_url(url: &str) -> Result<Url, OracleError> {
    let parsed =
        Url::parse(url).map_err(|e| OracleError::InvalidUrl(format!("Cannot parse URL: {e}")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(OracleError::InvalidUrl(format!(
            "Unsupported scheme '{scheme}', only http and https are allowed"
        ))),
    }
}

/// Error types for oracle fetching
#[derive(Debug, thiserror::Error)]
pub enum OracleError {
    #[error("HTTP request to {endpoint} failed: {source}")]
    RequestFailed {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("Failed to build oracle HTTP client: {0}")]
    ClientBuildFailed(#[source] reqwest::Error),

    #[error("Invalid oracle URL: {0}")]
    InvalidUrl(String),

    #[error("Invalid oracle response: {0}")]
    InvalidResponse(String),
}

fn request_failed(endpoint: &str, source: reqwest::Error) -> OracleError {
    OracleError::RequestFailed {
        endpoint: endpoint.to_owned(),
        source: source.without_url(),
    }
}

fn sanitized_endpoint(url: &Url) -> String {
    let mut endpoint = url.clone();
    let _ = endpoint.set_username("");
    let _ = endpoint.set_password(None);
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    endpoint.into()
}

/// JSON response format from an oracle endpoint.
/// Maps directly to `SignedContextV2` in the raindex contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleResponse {
    /// The signer address (EIP-712 signer of the context data)
    pub signer: Address,
    /// The EIP-712 domain separator the signer chose
    #[serde(rename = "domainSeparator")]
    pub domain_separator: FixedBytes<32>,
    /// The signed context data as bytes32[] values
    pub context: Vec<FixedBytes<32>>,
    /// The EIP-712 signature over `SignedContextV2(address signer,bytes32[] context)`
    /// under `domain_separator`
    pub signature: Bytes,
}

/// Error returned for one item in an `allowFailure=true` oracle batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, thiserror::Error)]
#[error("{error}: {detail}")]
pub struct OracleBatchItemError {
    /// Stable machine-readable error code supplied by the oracle.
    pub error: String,
    /// Human-readable details for this failed item.
    pub detail: String,
}

/// Result for one item in an `allowFailure=true` oracle batch.
pub type OracleBatchItemResult = Result<SignedContextV2, OracleBatchItemError>;

#[derive(Debug, Deserialize)]
#[serde(tag = "status", content = "body", rename_all = "lowercase")]
enum OracleBatchItemResponse {
    Ok(OracleResponse),
    Error(OracleBatchItemError),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OracleBatchResponse {
    Envelope(Vec<OracleBatchItemResponse>),
    Legacy(Vec<OracleResponse>),
}

impl From<OracleResponse> for SignedContextV2 {
    fn from(resp: OracleResponse) -> Self {
        SignedContextV2 {
            signer: resp.signer,
            domainSeparator: resp.domain_separator,
            context: resp.context,
            signature: resp.signature,
        }
    }
}

/// A reusable oracle HTTP client backed by a shared connection pool and
/// per-origin concurrency limit.
#[derive(Clone)]
pub(crate) struct OracleClient {
    client: Client,
    request_semaphores: Arc<OracleRequestSemaphores>,
    concurrency_limit: NonZeroUsize,
}

/// One batch oracle POST prepared for bounded execution.
pub(crate) struct OracleBatchRequest {
    url: String,
    body: Vec<u8>,
    expected_count: usize,
}

impl OracleBatchRequest {
    pub(crate) fn new(url: String, body: Vec<u8>, expected_count: usize) -> Self {
        Self {
            url,
            body,
            expected_count,
        }
    }
}

impl OracleClient {
    /// Build an oracle client with the default caller concurrency limit.
    pub(crate) fn new() -> Result<Self, OracleError> {
        Self::with_concurrency_limit(
            NonZeroUsize::new(ORACLE_REQUEST_CONCURRENCY_LIMIT)
                .expect("the default oracle request concurrency limit is non-zero"),
        )
    }

    /// Build an oracle client with a caller-specific concurrency limit.
    ///
    /// Requests also share the default per-origin ceiling with other clients.
    pub(crate) fn with_concurrency_limit(
        concurrency_limit: NonZeroUsize,
    ) -> Result<Self, OracleError> {
        let client = ORACLE_HTTP_CLIENT
            .get_or_try_init(build_http_client)
            .map_err(OracleError::ClientBuildFailed)?;
        Ok(Self {
            client: client.clone(),
            request_semaphores: ORACLE_REQUEST_SEMAPHORES.clone(),
            concurrency_limit,
        })
    }

    fn request_semaphore(&self, url: &Url) -> Arc<Semaphore> {
        let origin = url.origin().ascii_serialization();
        let mut semaphores = self
            .request_semaphores
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        semaphores.retain(|_, semaphore| semaphore.strong_count() > 0);

        semaphores
            .get(&origin)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| {
                let semaphore = Arc::new(Semaphore::new(ORACLE_REQUEST_CONCURRENCY_LIMIT));
                semaphores.insert(origin, Arc::downgrade(&semaphore));
                semaphore
            })
    }

    async fn with_request_permit<F, T>(&self, url: &Url, request: F) -> T
    where
        F: Future<Output = T>,
    {
        let request_semaphore = self.request_semaphore(url);
        let _permit = request_semaphore
            .acquire()
            .await
            .expect("oracle request semaphores are never closed");
        request.await
    }

    async fn fetch_json<T: DeserializeOwned>(
        &self,
        url: &Url,
        body: Vec<u8>,
    ) -> Result<T, OracleError> {
        let endpoint = sanitized_endpoint(url);
        self.with_request_permit(url, async {
            let response = self
                .client
                .post(url.clone())
                .header("Content-Type", "application/octet-stream")
                .body(body)
                .timeout(ORACLE_REQUEST_TIMEOUT)
                .send()
                .await
                .map_err(|source| request_failed(&endpoint, source))?
                .error_for_status()
                .map_err(|source| request_failed(&endpoint, source))?;

            response
                .json()
                .await
                .map_err(|source| request_failed(&endpoint, source))
        })
        .await
    }

    /// Fetch signed context for one pair.
    pub(crate) async fn fetch_signed_context(
        &self,
        url: &str,
        body: Vec<u8>,
    ) -> Result<SignedContextV2, OracleError> {
        let url = validate_oracle_url(url)?;
        self.fetch_signed_context_at(&url, body).await
    }

    async fn fetch_signed_context_at(
        &self,
        url: &Url,
        body: Vec<u8>,
    ) -> Result<SignedContextV2, OracleError> {
        let response: Vec<OracleResponse> = self.fetch_json(url, body).await?;

        let [response]: [OracleResponse; 1] =
            response
                .try_into()
                .map_err(|response: Vec<OracleResponse>| {
                    OracleError::InvalidResponse(format!(
                        "Expected 1 response, got {}",
                        response.len()
                    ))
                })?;
        Ok(response.into())
    }

    /// Fetch signed contexts from several independent batch endpoints.
    ///
    /// Each request is one HTTP POST containing all pair requests for its exact
    /// endpoint URL. Different endpoint batches execute concurrently through
    /// the caller and per-origin limits, while results remain aligned with the
    /// input batch order.
    pub(crate) async fn fetch_signed_context_batches(
        &self,
        requests: Vec<OracleBatchRequest>,
    ) -> Vec<Result<Vec<OracleBatchItemResult>, OracleError>> {
        let requests = requests
            .into_iter()
            .map(|request| async move {
                self.fetch_signed_context_batch_allow_failure(
                    &request.url,
                    request.body,
                    request.expected_count,
                )
                .await
            })
            .collect::<Vec<_>>();

        collect_bounded(requests, self.concurrency_limit).await
    }

    /// Fetch signed context for a batch body supported by a known-compatible
    /// endpoint.
    pub(crate) async fn fetch_signed_context_batch(
        &self,
        url: &str,
        body: Vec<u8>,
        expected_count: usize,
    ) -> Result<Vec<SignedContextV2>, OracleError> {
        let url = validate_oracle_url(url)?;
        let response: Vec<OracleResponse> = self.fetch_json(&url, body).await?;

        if response.len() != expected_count {
            return Err(OracleError::InvalidResponse(format!(
                "Expected {} oracle responses, got {}",
                expected_count,
                response.len()
            )));
        }

        Ok(response.into_iter().map(Into::into).collect())
    }

    /// Fetch independently fallible signed contexts from a compatible batch
    /// endpoint while preserving one result slot per request item.
    pub(crate) async fn fetch_signed_context_batch_allow_failure(
        &self,
        url: &str,
        body: Vec<u8>,
        expected_count: usize,
    ) -> Result<Vec<OracleBatchItemResult>, OracleError> {
        let mut url = validate_oracle_url(url)?;
        url.query_pairs_mut().append_pair("allowFailure", "true");
        let response: OracleBatchResponse = self.fetch_json(&url, body).await?;
        let response: Vec<OracleBatchItemResult> = match response {
            OracleBatchResponse::Envelope(items) => items
                .into_iter()
                .map(|item| match item {
                    OracleBatchItemResponse::Ok(response) => Ok(response.into()),
                    OracleBatchItemResponse::Error(error) => Err(error),
                })
                .collect(),
            OracleBatchResponse::Legacy(items) => {
                items.into_iter().map(|item| Ok(item.into())).collect()
            }
        };

        if response.len() != expected_count {
            return Err(OracleError::InvalidResponse(format!(
                "Expected {} oracle batch item responses, got {}",
                expected_count,
                response.len()
            )));
        }
        Ok(response)
    }
}

fn build_http_client() -> Result<Client, reqwest::Error> {
    Client::builder().build()
}

async fn collect_bounded<F, T>(futures: Vec<F>, limit: NonZeroUsize) -> Vec<T>
where
    F: Future<Output = T>,
{
    let mut pending = futures.into_iter().enumerate();
    let mut in_flight = FuturesUnordered::new();
    for (index, future) in pending.by_ref().take(limit.get()) {
        in_flight.push(await_indexed(index, future));
    }

    let mut results = Vec::new();
    while let Some(result) = in_flight.next().await {
        results.push(result);
        if let Some((index, future)) = pending.next() {
            in_flight.push(await_indexed(index, future));
        }
    }

    results.sort_unstable_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

async fn await_indexed<F, T>(index: usize, future: F) -> (usize, T)
where
    F: Future<Output = T>,
{
    (index, future.await)
}

/// Encode the POST body for a single oracle request.
///
/// The body is `abi.encode(OrderV4, uint256 inputIOIndex, uint256 outputIOIndex, address counterparty)`.
pub fn encode_oracle_body(
    order: &OrderV4,
    input_io_index: u32,
    output_io_index: u32,
    counterparty: Address,
) -> Vec<u8> {
    (
        order.clone(),
        U256::from(input_io_index),
        U256::from(output_io_index),
        counterparty,
    )
        .abi_encode()
}

/// Encode the POST body for a batch oracle request.
///
/// The body is `abi.encode((OrderV4, uint256 inputIOIndex, uint256 outputIOIndex, address counterparty)[])`.
pub fn encode_oracle_body_batch(requests: Vec<(&OrderV4, u32, u32, Address)>) -> Vec<u8> {
    let tuples: Vec<_> = requests
        .into_iter()
        .map(|(order, input_io_index, output_io_index, counterparty)| {
            (
                order.clone(),
                U256::from(input_io_index),
                U256::from(output_io_index),
                counterparty,
            )
        })
        .collect();

    tuples.abi_encode()
}

/// Fetch signed context from an oracle endpoint via POST (single request).
///
/// The endpoint receives an ABI-encoded body containing the order details
/// that will be used for calculateOrderIO:
/// `abi.encode(OrderV4, uint256 inputIOIndex, uint256 outputIOIndex, address counterparty)`
///
/// The endpoint must respond with a JSON array containing exactly one `OracleResponse`.
pub async fn fetch_signed_context(
    url: &str,
    body: Vec<u8>,
) -> Result<SignedContextV2, OracleError> {
    OracleClient::new()?.fetch_signed_context(url, body).await
}

/// Fetch signed context from an oracle endpoint via POST (batch request).
///
/// The endpoint receives an ABI-encoded body containing an array of order details:
/// `abi.encode((OrderV4, uint256 inputIOIndex, uint256 outputIOIndex, address counterparty)[])`
///
/// The endpoint must respond with a JSON array of `OracleResponse` objects.
/// The response array length must match the request array length.
pub async fn fetch_signed_context_batch(
    url: &str,
    body: Vec<u8>,
    expected_count: usize,
) -> Result<Vec<SignedContextV2>, OracleError> {
    OracleClient::new()?
        .fetch_signed_context_batch(url, body, expected_count)
        .await
}

/// Fetch signed context for a batch using the oracle's per-item failure
/// envelope. Transport, HTTP, JSON-shape, and result-count failures remain
/// request-level errors; valid item errors remain aligned with their request.
pub async fn fetch_signed_context_batch_allow_failure(
    url: &str,
    body: Vec<u8>,
    expected_count: usize,
) -> Result<Vec<OracleBatchItemResult>, OracleError> {
    OracleClient::new()?
        .fetch_signed_context_batch_allow_failure(url, body, expected_count)
        .await
}

/// Extract the oracle URL from an SgOrder's meta, if present.
///
/// Parses the meta bytes and looks for a `RaindexSignedContextOracleV1` entry.
/// Returns `None` if meta is absent, unparseable, or doesn't contain an oracle entry.
pub fn extract_oracle_url(order: &SgOrder) -> Option<String> {
    use rain_metadata::types::raindex_signed_context_oracle::RaindexSignedContextOracleV1;
    use rain_metadata::RainMetaDocumentV1Item;

    let meta = order.meta.as_ref()?;
    let decoded = alloy::hex::decode(&meta.0).ok()?;
    let items = RainMetaDocumentV1Item::cbor_decode(&decoded).ok()?;
    let oracle = RaindexSignedContextOracleV1::find_in_items(&items).ok()??;
    Some(oracle.url().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{address, FixedBytes};
    #[cfg(not(target_family = "wasm"))]
    use httpmock::{Method::POST, MockServer};
    use raindex_bindings::IRaindexV7::{EvaluableV4, OrderV4, IOV2};
    #[cfg(not(target_family = "wasm"))]
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn test_oracle_response_to_signed_context() {
        let ctx_val = FixedBytes::<32>::from([0x2a; 32]);
        let response = OracleResponse {
            signer: address!("0x1234567890123456789012345678901234567890"),
            domain_separator: FixedBytes::<32>::repeat_byte(0xd5),
            context: vec![ctx_val],
            signature: Bytes::from(vec![0xaa, 0xbb, 0xcc]),
        };

        let signed: SignedContextV2 = response.into();
        assert_eq!(
            signed.signer,
            address!("0x1234567890123456789012345678901234567890")
        );
        assert_eq!(signed.domainSeparator, FixedBytes::<32>::repeat_byte(0xd5));
        assert_eq!(signed.context.len(), 1);
        assert_eq!(signed.context[0], ctx_val);
        assert_eq!(signed.signature, Bytes::from(vec![0xaa, 0xbb, 0xcc]));
    }

    #[test]
    fn test_oracle_response_json_requires_domain_separator() {
        let domain = format!("0x{}", "d5".repeat(32));
        let with_domain: OracleResponse = serde_json::from_value(serde_json::json!({
            "signer": "0x1234567890123456789012345678901234567890",
            "domainSeparator": domain,
            "context": [format!("0x{}", "2a".repeat(32))],
            "signature": "0xaabbcc",
        }))
        .unwrap();
        assert_eq!(
            with_domain.domain_separator,
            FixedBytes::<32>::repeat_byte(0xd5)
        );
        assert_eq!(
            serde_json::to_value(&with_domain).unwrap()["domainSeparator"],
            serde_json::json!(domain)
        );

        let without_domain = serde_json::from_value::<OracleResponse>(serde_json::json!({
            "signer": "0x1234567890123456789012345678901234567890",
            "context": [format!("0x{}", "2a".repeat(32))],
            "signature": "0xaabbcc",
        }));
        assert!(without_domain.is_err());
    }

    #[test]
    fn test_encode_oracle_body_single() {
        let order = create_test_order();
        let body = encode_oracle_body(
            &order,
            1,
            2,
            address!("0x1111111111111111111111111111111111111111"),
        );
        assert!(!body.is_empty());
    }

    #[test]
    fn test_encode_oracle_body_batch() {
        let order1 = create_test_order();
        let order2 = create_test_order();

        let requests = vec![
            (
                &order1,
                1,
                2,
                address!("0x1111111111111111111111111111111111111111"),
            ),
            (
                &order2,
                3,
                4,
                address!("0x2222222222222222222222222222222222222222"),
            ),
        ];

        let body = encode_oracle_body_batch(requests);
        assert!(!body.is_empty());

        // Batch encoding should be different from single encoding
        let single_body = encode_oracle_body(
            &order1,
            1,
            2,
            address!("0x1111111111111111111111111111111111111111"),
        );
        assert_ne!(body, single_body);
    }

    #[tokio::test]
    async fn test_fetch_signed_context_invalid_url() {
        let result = fetch_signed_context("not-a-url", vec![]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_signed_context_unreachable() {
        let result = fetch_signed_context("http://127.0.0.1:1/oracle", vec![]).await;
        assert!(result.is_err());
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_fetch_signed_context_error_redacts_url() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/oracle");
                then.status(503);
            })
            .await;
        let secret = "do-not-log-this-api-key";
        let url = format!("{}?api_key={secret}", server.url("/oracle"));

        let error = fetch_signed_context(&url, vec![]).await.unwrap_err();
        let message = error.to_string();
        let parsed_url = Url::parse(&url).unwrap();

        assert!(message.contains("503"));
        assert!(message.contains(parsed_url.host_str().unwrap()));
        assert!(message.contains("/oracle"));
        assert!(!message.contains(secret));
        assert!(!message.contains(&url));
        mock.assert_async().await;
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_fetch_signed_context_batches_preserves_item_and_request_failures() {
        let server = MockServer::start_async().await;
        let first_response = OracleResponse {
            signer: address!("0x1111111111111111111111111111111111111111"),
            domain_separator: FixedBytes::<32>::repeat_byte(0xd5),
            context: vec![FixedBytes::with_last_byte(1)],
            signature: vec![0xaa].into(),
        };
        let third_response = OracleResponse {
            signer: address!("0x3333333333333333333333333333333333333333"),
            domain_separator: FixedBytes::<32>::repeat_byte(0xd5),
            context: vec![FixedBytes::with_last_byte(3)],
            signature: vec![0xcc].into(),
        };
        let first_mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/first")
                    .query_param("allowFailure", "true");
                then.status(200).json_body(json!([
                    {"status": "ok", "body": first_response.clone()},
                    {
                        "status": "error",
                        "body": {"error": "service_unavailable", "detail": "pair unavailable"}
                    }
                ]));
            })
            .await;
        let failing_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/failing");
                then.status(503);
            })
            .await;
        let third_mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/third")
                    .query_param("apiKey", "kept")
                    .query_param("allowFailure", "true");
                then.status(200).json_body(json!([
                    {"status": "ok", "body": third_response.clone()}
                ]));
            })
            .await;
        let requests = vec![
            OracleBatchRequest::new(server.url("/first"), vec![], 2),
            OracleBatchRequest::new(server.url("/failing"), vec![], 1),
            OracleBatchRequest::new(format!("{}?apiKey=kept", server.url("/third")), vec![], 1),
        ];

        let results = OracleClient::new()
            .unwrap()
            .fetch_signed_context_batches(requests)
            .await;

        assert_eq!(results.len(), 3);
        assert_eq!(
            results[0].as_ref().unwrap()[0].as_ref().unwrap().signer,
            address!("0x1111111111111111111111111111111111111111")
        );
        assert_eq!(
            results[0].as_ref().unwrap()[1].as_ref().unwrap_err(),
            &OracleBatchItemError {
                error: "service_unavailable".to_string(),
                detail: "pair unavailable".to_string(),
            }
        );
        assert!(results[1].is_err());
        assert_eq!(
            results[2].as_ref().unwrap()[0].as_ref().unwrap().signer,
            address!("0x3333333333333333333333333333333333333333")
        );
        first_mock.assert_async().await;
        failing_mock.assert_async().await;
        third_mock.assert_async().await;
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_fetch_signed_context_batch_allow_failure_all_success() {
        let server = MockServer::start_async().await;
        let first = OracleResponse {
            signer: address!("0x1111111111111111111111111111111111111111"),
            domain_separator: FixedBytes::<32>::repeat_byte(0xd5),
            context: vec![FixedBytes::with_last_byte(1)],
            signature: vec![0xaa].into(),
        };
        let second = OracleResponse {
            signer: address!("0x2222222222222222222222222222222222222222"),
            domain_separator: FixedBytes::<32>::repeat_byte(0xd5),
            context: vec![FixedBytes::with_last_byte(2)],
            signature: vec![0xbb].into(),
        };
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/oracle")
                    .query_param("allowFailure", "true");
                then.status(200).json_body(json!([
                    {"status": "ok", "body": first},
                    {"status": "ok", "body": second}
                ]));
            })
            .await;

        let results = fetch_signed_context_batch_allow_failure(&server.url("/oracle"), vec![], 2)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].as_ref().unwrap().signer,
            address!("0x1111111111111111111111111111111111111111")
        );
        assert_eq!(
            results[1].as_ref().unwrap().signer,
            address!("0x2222222222222222222222222222222222222222")
        );
        mock.assert_async().await;
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_fetch_signed_context_batch_allow_failure_accepts_legacy_success_shape() {
        let server = MockServer::start_async().await;
        let response = OracleResponse {
            signer: address!("0x1111111111111111111111111111111111111111"),
            domain_separator: FixedBytes::<32>::repeat_byte(0xd5),
            context: vec![FixedBytes::with_last_byte(1)],
            signature: vec![0xaa].into(),
        };
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/oracle")
                    .query_param("allowFailure", "true");
                then.status(200).json_body_obj(&vec![response]);
            })
            .await;

        let results = fetch_signed_context_batch_allow_failure(&server.url("/oracle"), vec![], 1)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].as_ref().unwrap().signer,
            address!("0x1111111111111111111111111111111111111111")
        );
        mock.assert_async().await;
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_fetch_signed_context_batch_allow_failure_all_errors() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/oracle")
                    .query_param("allowFailure", "true");
                then.status(200).json_body(json!([
                    {"status": "error", "body": {"error": "bad_request", "detail": "bad pair"}},
                    {"status": "error", "body": {"error": "internal_error", "detail": "signer failed"}}
                ]));
            })
            .await;

        let results = fetch_signed_context_batch_allow_failure(&server.url("/oracle"), vec![], 2)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_ref().unwrap_err().error, "bad_request");
        assert_eq!(results[1].as_ref().unwrap_err().error, "internal_error");
        mock.assert_async().await;
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_fetch_signed_context_batch_allow_failure_rejects_malformed_and_count_mismatch() {
        let server = MockServer::start_async().await;
        let malformed = server
            .mock_async(|when, then| {
                when.method(POST).path("/malformed");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(r#"[{"status":"ok","body":{"unexpected":true}}]"#);
            })
            .await;
        let mismatch = server
            .mock_async(|when, then| {
                when.method(POST).path("/mismatch");
                then.status(200).json_body(json!([]));
            })
            .await;

        let malformed_error =
            fetch_signed_context_batch_allow_failure(&server.url("/malformed"), vec![], 1)
                .await
                .unwrap_err();
        assert!(matches!(malformed_error, OracleError::RequestFailed { .. }));

        let mismatch_error =
            fetch_signed_context_batch_allow_failure(&server.url("/mismatch"), vec![], 1)
                .await
                .unwrap_err();
        assert!(matches!(mismatch_error, OracleError::InvalidResponse(_)));
        malformed.assert_async().await;
        mismatch.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_signed_context_batch_invalid_url() {
        let result = fetch_signed_context_batch("not-a-url", vec![], 0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_signed_context_batch_unreachable() {
        let result = fetch_signed_context_batch("http://127.0.0.1:1/oracle", vec![], 0).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_oracle_url_rejects_non_http() {
        assert!(validate_oracle_url("ftp://example.com").is_err());
        assert!(validate_oracle_url("javascript:alert(1)").is_err());
        assert!(validate_oracle_url("file:///etc/passwd").is_err());
        assert!(validate_oracle_url("data:text/html,<h1>hi</h1>").is_err());
    }

    #[test]
    fn test_validate_oracle_url_accepts_http() {
        assert!(validate_oracle_url("http://localhost:8080/oracle").is_ok());
        assert!(validate_oracle_url("https://oracle.example.com/context").is_ok());
    }

    #[tokio::test]
    async fn test_collect_bounded_preserves_positions_and_limits_concurrency() {
        const REQUEST_COUNT: usize = 17;
        const TEST_LIMIT: usize = 3;

        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Semaphore::new(0));
        let requests = (0..REQUEST_COUNT).map(|index| {
            let active = active.clone();
            let maximum = maximum.clone();
            let started = started.clone();
            let gate = gate.clone();
            async move {
                let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(active_now, Ordering::SeqCst);
                let started_now = started.fetch_add(1, Ordering::SeqCst) + 1;
                if started_now == TEST_LIMIT {
                    gate.add_permits(REQUEST_COUNT);
                }

                let _permit = gate.acquire().await.unwrap();
                active.fetch_sub(1, Ordering::SeqCst);

                if index == 5 {
                    Err("expected failure")
                } else {
                    Ok(index)
                }
            }
        });

        let results =
            collect_bounded(requests.collect(), NonZeroUsize::new(TEST_LIMIT).unwrap()).await;

        assert_eq!(maximum.load(Ordering::SeqCst), TEST_LIMIT);
        assert_eq!(started.load(Ordering::SeqCst), REQUEST_COUNT);
        assert_eq!(results.len(), REQUEST_COUNT);
        assert_eq!(results[0], Ok(0));
        assert_eq!(results[5], Err("expected failure"));
        assert_eq!(results[6], Ok(6));
        assert_eq!(results[16], Ok(16));
    }

    #[tokio::test]
    async fn test_collect_bounded_does_not_block_behind_slow_first_request() {
        const REQUEST_COUNT: usize = 8;
        const TEST_LIMIT: usize = 3;

        let slow_request_gate = Arc::new(Semaphore::new(0));
        let later_request_started = Arc::new(Semaphore::new(0));
        let requests = (0..REQUEST_COUNT).map(|index| {
            let slow_request_gate = slow_request_gate.clone();
            let later_request_started = later_request_started.clone();
            async move {
                if index == 0 {
                    let _permit = slow_request_gate.acquire().await.unwrap();
                } else if index == TEST_LIMIT {
                    later_request_started.add_permits(1);
                }
                index
            }
        });
        let results = collect_bounded(requests.collect(), NonZeroUsize::new(TEST_LIMIT).unwrap());
        tokio::pin!(results);

        tokio::select! {
            permit = later_request_started.acquire() => {
                assert!(permit.is_ok(), "later request signal must remain open");
            }
            _ = &mut results => {
                panic!("all requests completed while the first request was still gated");
            }
        }

        slow_request_gate.add_permits(1);
        assert_eq!(results.await, (0..REQUEST_COUNT).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn test_oracle_clients_share_per_origin_request_limit() {
        const REQUEST_COUNT: usize = 17;

        let global_client = OracleClient::new().unwrap();
        let second_global_client = OracleClient::new().unwrap();
        assert!(Arc::ptr_eq(
            &global_client.request_semaphores,
            &second_global_client.request_semaphores
        ));

        let same_origin = Url::parse("https://oracle.example.com/first").unwrap();
        let same_origin_other_path = Url::parse("https://oracle.example.com/second").unwrap();
        let other_origin = Url::parse("https://other.example.com/oracle").unwrap();
        let first_semaphore = global_client.request_semaphore(&same_origin);
        let same_origin_semaphore = second_global_client.request_semaphore(&same_origin_other_path);
        let other_origin_semaphore = global_client.request_semaphore(&other_origin);
        assert!(Arc::ptr_eq(&first_semaphore, &same_origin_semaphore));
        assert!(!Arc::ptr_eq(&first_semaphore, &other_origin_semaphore));

        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Semaphore::new(0));
        let requests = (0..REQUEST_COUNT).map(|index| {
            let client = if index % 2 == 0 {
                global_client.clone()
            } else {
                second_global_client.clone()
            };
            let same_origin = same_origin.clone();
            let active = active.clone();
            let maximum = maximum.clone();
            let started = started.clone();
            let gate = gate.clone();
            async move {
                client
                    .with_request_permit(&same_origin, async move {
                        let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(active_now, Ordering::SeqCst);
                        let started_now = started.fetch_add(1, Ordering::SeqCst) + 1;
                        if started_now == ORACLE_REQUEST_CONCURRENCY_LIMIT {
                            gate.add_permits(REQUEST_COUNT);
                        }

                        let _permit = gate.acquire().await.unwrap();
                        active.fetch_sub(1, Ordering::SeqCst);
                    })
                    .await;
            }
        });

        collect_bounded(
            requests.collect(),
            NonZeroUsize::new(REQUEST_COUNT).unwrap(),
        )
        .await;

        assert_eq!(
            maximum.load(Ordering::SeqCst),
            ORACLE_REQUEST_CONCURRENCY_LIMIT
        );
        assert_eq!(started.load(Ordering::SeqCst), REQUEST_COUNT);
    }

    #[test]
    fn test_oracle_client_accepts_caller_concurrency_limit() {
        let limit = NonZeroUsize::new(3).unwrap();
        let client = OracleClient::with_concurrency_limit(limit).unwrap();

        assert_eq!(client.concurrency_limit, limit);
    }

    fn create_test_order() -> OrderV4 {
        OrderV4 {
            owner: address!("0x0000000000000000000000000000000000000000"),
            evaluable: EvaluableV4 {
                interpreter: address!("0x0000000000000000000000000000000000000000"),
                store: address!("0x0000000000000000000000000000000000000000"),
                bytecode: Bytes::new(),
            },
            validInputs: vec![IOV2 {
                token: address!("0x0000000000000000000000000000000000000000"),
                vaultId: FixedBytes::<32>::ZERO,
            }],
            validOutputs: vec![IOV2 {
                token: address!("0x0000000000000000000000000000000000000000"),
                vaultId: FixedBytes::<32>::ZERO,
            }],
            nonce: FixedBytes::<32>::ZERO,
        }
    }
}
