use alloy::primitives::{Address, Bytes, B256, U256};
use alloy::providers::Provider;
use alloy::rpc::json_rpc::{Id, RequestMeta};
use alloy::rpc::types::Filter;
use alloy::transports::TransportError;
use raindex_bindings::provider::{mk_read_provider, ReadProvider, ReadProviderError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use thiserror::Error;
use url::Url;

#[derive(Clone)]
pub struct RpcClient {
    chain_id: Option<u32>,
    rpc_urls: Vec<Url>,
    provider: Arc<ReadProvider>,
}

/// Minimal block view required for timestamp backfilling.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockResponse {
    pub timestamp: U256,
    pub hash: B256,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// Typed view of a single log returned by HyperSync's `eth_getLogs`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntryResponse {
    pub address: Address,
    pub topics: Vec<Bytes>,
    pub data: Bytes,
    pub block_number: U256,
    pub block_timestamp: Option<U256>,
    pub transaction_hash: B256,
    pub transaction_index: String,
    pub block_hash: B256,
    pub log_index: U256,
    pub removed: bool,
}

impl std::fmt::Debug for RpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted_urls: Vec<String> = self
            .rpc_urls
            .iter()
            .map(|url| {
                let url_str = url.as_str();
                if let Some(last_slash) = url_str.rfind('/') {
                    format!("{}/***", &url_str[..last_slash])
                } else {
                    "***".to_string()
                }
            })
            .collect();

        f.debug_struct("RpcClient")
            .field("chain_id", &self.chain_id)
            .field("rpc_urls", &redacted_urls)
            .finish()
    }
}

impl RpcClient {
    pub fn new_with_urls(urls: Vec<Url>) -> Result<Self, RpcClientError> {
        if urls.is_empty() {
            return Err(RpcClientError::Config {
                message: "at least one RPC URL is required".to_string(),
            });
        }

        let provider = Arc::new(mk_read_provider(&urls)?);
        Ok(Self {
            chain_id: None,
            rpc_urls: urls,
            provider,
        })
    }

    pub fn new_with_hyper_rpc(chain_id: u32, api_token: &str) -> Result<Self, RpcClientError> {
        let url = Self::build_hyper_url(chain_id, api_token)?;
        let provider = Arc::new(mk_read_provider(std::slice::from_ref(&url))?);
        Ok(Self {
            chain_id: Some(chain_id),
            rpc_urls: vec![url],
            provider,
        })
    }

    pub fn build_hyper_url(chain_id: u32, api_token: &str) -> Result<Url, RpcClientError> {
        let url = format!("https://{chain_id}.rpc.hypersync.xyz/{api_token}");
        Ok(Url::parse(&url)?)
    }

    pub fn rpc_urls(&self) -> &[Url] {
        &self.rpc_urls
    }

    pub async fn get_latest_block_number(&self) -> Result<u64, RpcClientError> {
        let block_hex = self
            .provider
            .client()
            .request::<Vec<()>, String>("eth_blockNumber", Vec::new())
            .map_meta(set_request_id)
            .await
            .map_err(|err| Self::map_transport_error(err, Some("Getting latest block")))?;

        let block_hex = block_hex.strip_prefix("0x").unwrap_or(&block_hex);
        let block_number = u64::from_str_radix(block_hex, 16)?;
        Ok(block_number)
    }

    pub async fn get_logs(&self, filter: &Filter) -> Result<Vec<LogEntryResponse>, RpcClientError> {
        let params = serde_json::json!([filter]);
        self.provider
            .client()
            .request::<_, Vec<LogEntryResponse>>("eth_getLogs", params)
            .map_meta(set_request_id)
            .await
            .map_err(|err| Self::map_transport_error(err, None))
    }

    pub async fn get_block_by_number(
        &self,
        block_number: u64,
    ) -> Result<Option<BlockResponse>, RpcClientError> {
        let block_hex = format!("0x{:x}", block_number);
        let params = serde_json::json!([block_hex, false]);

        self.provider
            .client()
            .request::<_, Option<BlockResponse>>("eth_getBlockByNumber", params)
            .map_meta(set_request_id)
            .await
            .map_err(|err| Self::map_transport_error(err, None))
    }

    #[cfg(test)]
    pub fn mock() -> Self {
        let rpc_urls = vec![Url::parse("https://mock-url.com").unwrap()];
        let provider = mk_read_provider(&rpc_urls).expect("failed to update provider");
        RpcClient {
            chain_id: None,
            rpc_urls,
            provider: Arc::new(provider),
        }
    }

    #[cfg(all(test, not(target_family = "wasm")))]
    pub(crate) fn update_rpc_urls(&mut self, urls: Vec<Url>) {
        let provider = mk_read_provider(&urls).expect("failed to update provider");
        self.rpc_urls = urls;
        self.provider = Arc::new(provider);
    }

    fn map_transport_error(err: TransportError, context: Option<&str>) -> RpcClientError {
        match err {
            TransportError::ErrorResp(resp) => {
                if resp.code == -32090 {
                    return RpcClientError::RateLimited {
                        message: resp.message.to_string(),
                    };
                }
                let message = if let Some(ctx) = context {
                    format!("{}: {}", ctx, resp)
                } else {
                    resp.to_string()
                };
                RpcClientError::RpcError { message }
            }
            TransportError::NullResp => RpcClientError::MissingField {
                field: "result".to_string(),
            },
            TransportError::DeserError { err, text } => {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    if let Some(error_value) = value.get("error") {
                        return RpcClientError::RpcError {
                            message: error_value.to_string(),
                        };
                    }
                }
                RpcClientError::JsonSerialization(err)
            }
            other => RpcClientError::Transport(other),
        }
    }
}

fn set_request_id(mut meta: RequestMeta) -> RequestMeta {
    meta.id = Id::from(1_u64);
    meta
}

#[derive(Debug, Error)]
pub enum RpcClientError {
    #[error("Unsupported chain ID: {chain_id}")]
    UnsupportedChainId { chain_id: u32 },

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Provider construction failed: {0}")]
    ProviderConstruction(#[from] ReadProviderError),

    #[error("Transport error: {0}")]
    Transport(TransportError),

    #[error("RPC error: {message}")]
    RpcError { message: String },

    #[error("Rate limited: {message}")]
    RateLimited { message: String },

    #[error("Missing expected field: {field}")]
    MissingField { field: String },

    #[error("Invalid hex format: {0}")]
    HexParseError(#[from] std::num::ParseIntError),

    #[error("JSON serialization error: {0}")]
    JsonSerialization(#[from] serde_json::Error),

    #[error("Configuration error: {message}")]
    Config { message: String },

    #[error("Invalid block range: start {start} > end {end}")]
    InvalidBlockRange { start: u64, end: u64 },
}

impl From<TransportError> for RpcClientError {
    fn from(err: TransportError) -> Self {
        RpcClient::map_transport_error(err, None)
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use alloy::{hex, primitives::b256};
    use httpmock::MockServer;
    use serde_json::json;

    fn sample_block_response_with_hash(number: &str, timestamp: &str, hash: &str) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "mixHash": "0xmix",
                "difficulty": "0x1",
                "extraData": "0xextra",
                "gasLimit": "0xffff",
                "gasUsed": "0xff",
                "hash": hash,
                "logsBloom": "0x0",
                "miner": "0xminer",
                "nonce": "0xnonce",
                "number": number,
                "parentHash": "0xparent",
                "receiptsRoot": "0xreceipts",
                "sha3Uncles": "0xsha3",
                "size": "0x1",
                "stateRoot": "0xstate",
                "timestamp": timestamp,
                "totalDifficulty": "0x2",
                "transactionsRoot": "0xtransactions",
                "uncles": [],
                "transactions": [],
            }
        })
        .to_string()
    }

    fn logs_response_body(logs: serde_json::Value) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": logs,
        })
        .to_string()
    }

    fn sample_log_entry(block_number: &str) -> serde_json::Value {
        json!({
            "address": "0x0000000000000000000000000000000000000123",
            "topics": ["0x0000000000000000000000000000000000000000000000000000000000000abc"],
            "data": "0xdeadbeef",
            "blockNumber": block_number,
            "blockTimestamp": "0x05",
            "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
            "transactionIndex": "0x0",
            "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
            "logIndex": "0x00",
            "removed": false
        })
    }

    #[test]
    fn test_build_hyper_url_uses_numeric_chain_id_alias() {
        for chain_id in [1, 999, 8453, 9999] {
            let url = RpcClient::build_hyper_url(chain_id, "test_token").unwrap();
            assert_eq!(
                url.as_str(),
                format!("https://{chain_id}.rpc.hypersync.xyz/test_token")
            );
        }
    }

    #[tokio::test]
    async fn test_get_latest_block_number_valid_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .header("content-type", "application/json")
                .json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_blockNumber",
                    "params": []
                }));
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"jsonrpc": "2.0", "id": 1, "result": "0x1b4"}"#);
        });

        let mut client =
            RpcClient::new_with_urls(vec![Url::parse(&server.base_url()).unwrap()]).unwrap();
        let result = client.get_latest_block_number().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 436);

        mock.assert();

        // Update URLs to ensure debug path works.
        client.update_rpc_urls(vec![Url::parse(&server.base_url()).unwrap()]);
    }
    #[tokio::test]
    async fn test_rate_limited_error_code_detected() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST);
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"jsonrpc": "2.0", "id": 1, "error": {"code": -32090, "message": "rate limited"}}"#);
        });

        let client =
            RpcClient::new_with_urls(vec![Url::parse(&server.base_url()).unwrap()]).unwrap();
        let result = client.get_latest_block_number().await;
        match result.unwrap_err() {
            RpcClientError::RateLimited { message } => {
                assert!(
                    message.contains("rate limited"),
                    "expected 'rate limited' in message, got: {message}"
                );
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }

        mock.assert();
    }

    #[tokio::test]
    async fn test_get_latest_block_number_rpc_error() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST);
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"jsonrpc": "2.0", "id": 1, "error": {"code": -32602, "message": "Invalid params"}}"#);
        });

        let client =
            RpcClient::new_with_urls(vec![Url::parse(&server.base_url()).unwrap()]).unwrap();
        let result = client.get_latest_block_number().await;
        assert!(matches!(
            result.unwrap_err(),
            RpcClientError::RpcError { .. }
        ));

        mock.assert();
    }

    #[tokio::test]
    async fn test_get_block_by_number_ok() {
        let server = MockServer::start();
        let expected_hash =
            b256!("0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899");
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .header("content-type", "application/json")
                .json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_getBlockByNumber",
                    "params": ["0x64", false]
                }));
            then.status(200)
                .header("content-type", "application/json")
                .body(sample_block_response_with_hash(
                    "0x64",
                    "0x64b8c123",
                    "0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
                ));
        });

        let client =
            RpcClient::new_with_urls(vec![Url::parse(&server.base_url()).unwrap()]).unwrap();
        let response = client.get_block_by_number(100).await.unwrap();
        let block = response.expect("block response present");
        assert_eq!(block.timestamp, U256::from(0x64b8c123u64));
        assert_eq!(block.hash, expected_hash);
        assert_eq!(
            format!("{:#x}", block.hash),
            "0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
        );

        mock.assert();
    }

    #[test]
    fn block_response_includes_hash_and_extra_fields() {
        let expected_hash =
            b256!("0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899");
        let body =
            sample_block_response_with_hash("0x2a", "0x5f5e100", &format!("{:#x}", expected_hash));
        let parsed: serde_json::Value =
            serde_json::from_str(&body).expect("valid json for block response");
        let block: BlockResponse = serde_json::from_value(parsed["result"].clone())
            .expect("block response should deserialize");

        assert_eq!(block.hash, expected_hash);
        assert_eq!(block.timestamp, U256::from(0x5f5e100u64));

        let mix_hash = block
            .extra
            .get("mixHash")
            .and_then(|value| value.as_str())
            .expect("flattened field mixHash present");
        assert_eq!(mix_hash, "0xmix");

        assert_eq!(
            format!("{:#x}", block.hash),
            "0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
        );
        assert_eq!(block.hash.len(), 32);
    }

    #[tokio::test]
    async fn test_get_logs_ok() {
        use alloy::primitives::{Address, B256};
        use alloy::rpc::types::Filter;
        use serde_json::json;
        use std::str::FromStr;

        let server = MockServer::start();
        let log_entry = sample_log_entry("0x64");

        // Build typed inputs and expected wire values
        let address = Address::from_str("0x0000000000000000000000000000000000000123").unwrap();
        let expected_address = format!("{:#x}", address);
        let mut topic_bytes = [0u8; 32];
        topic_bytes[30] = 0x0a;
        topic_bytes[31] = 0xbc;
        let topic = B256::from(topic_bytes);
        let expected_topic = format!("0x{}", hex::encode(topic.as_slice()));

        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .header("content-type", "application/json")
                .body_contains("\"eth_getLogs\"")
                .body_contains("\"fromBlock\":\"0x1\"")
                .body_contains("\"toBlock\":\"0x2\"")
                .body_contains(&expected_address)
                .body_contains(&expected_topic);
            then.status(200)
                .header("content-type", "application/json")
                .body(logs_response_body(json!([log_entry])));
        });

        let client =
            RpcClient::new_with_urls(vec![Url::parse(&server.base_url()).unwrap()]).unwrap();

        let filter_json = json!({
            "fromBlock": "0x1",
            "toBlock": "0x2",
            "address": expected_address,
            "topics": [[expected_topic]],
        });
        let filter: Filter = serde_json::from_value(filter_json).unwrap();

        let logs = client.get_logs(&filter).await.unwrap();

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].block_number, U256::from(0x64));

        mock.assert();
    }
}
