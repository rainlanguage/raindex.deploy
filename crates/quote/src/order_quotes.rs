#[cfg(test)]
use crate::injector::NoopInjector;
#[cfg(not(target_family = "wasm"))]
use crate::rpc::{with_rpc_timeout, RPC_ATTEMPT_TIMEOUT_MS};
use crate::{
    error::Error,
    injector::SignedContextInjector,
    oracle::{OracleBatchRequest, OracleClient, ORACLE_REQUEST_CONCURRENCY_LIMIT},
    quote::{BatchQuoteTarget, QuoteTarget},
    OrderQuoteValue,
};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use raindex_bindings::provider::mk_read_provider;
use raindex_bindings::IRaindexV7::{OrderV4, QuoteV3, SignedContextV2};
use raindex_subgraph_client::types::common::SgOrder;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr};
use tracing::{debug, info, warn};
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::prelude::js_sys::Date;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::{impl_wasm_traits, prelude::*};

struct QuoteTiming {
    #[cfg(not(target_family = "wasm"))]
    started_at: std::time::Instant,
    #[cfg(target_family = "wasm")]
    started_at_ms: f64,
}

impl QuoteTiming {
    fn now() -> Self {
        Self {
            #[cfg(not(target_family = "wasm"))]
            started_at: std::time::Instant::now(),
            #[cfg(target_family = "wasm")]
            started_at_ms: Date::now(),
        }
    }

    fn elapsed_ms(&self) -> u64 {
        #[cfg(not(target_family = "wasm"))]
        {
            self.started_at.elapsed().as_millis() as u64
        }

        #[cfg(target_family = "wasm")]
        {
            let elapsed = Date::now() - self.started_at_ms;
            if elapsed.is_finite() && elapsed > 0.0 {
                elapsed as u64
            } else {
                0
            }
        }
    }
}

struct QuotePairPreparation {
    pair: Pair,
    quote_target: QuoteTarget,
    oracle_url: Option<String>,
}

struct OracleBatchItem {
    pair_preparation_index: usize,
}

struct OracleBatchPreparation {
    url: String,
    items: Vec<OracleBatchItem>,
}

struct QuotedPairMetadata {
    slot_index: usize,
    pair: Pair,
}

fn pair_is_selected(
    selected_pairs: Option<&[Vec<Pair>]>,
    order_index: usize,
    input_index: u32,
    output_index: u32,
) -> bool {
    selected_pairs
        .and_then(|pairs| pairs.get(order_index))
        .is_none_or(|pairs| {
            pairs
                .iter()
                .any(|pair| pair.input_index == input_index && pair.output_index == output_index)
        })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct BatchOrderQuotesResponse {
    pub pair: Pair,
    pub block_number: u64,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub data: Option<OrderQuoteValue>,
    pub success: bool,
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub error: Option<String>,
    /// Composed signed context that was sent with the quote RPC: any
    /// oracle-fetched entries first, followed by injector-contributed entries
    /// (composition order: `[oracle..., injected...]`). This is propagated so
    /// downstream candidate construction can reuse the same context that the
    /// quote call saw, rather than re-fetching or re-composing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(target_family = "wasm", tsify(optional))]
    pub signed_context: Vec<SignedContextV2>,
}
#[cfg(target_family = "wasm")]
impl_wasm_traits!(BatchOrderQuotesResponse);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[cfg_attr(target_family = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub pair_name: String,
    pub input_index: u32,
    pub output_index: u32,
}
#[cfg(target_family = "wasm")]
impl_wasm_traits!(Pair);

/// Get order quotes, automatically fetching signed oracle context from order
/// meta and appending any caller-supplied injector contexts.
///
/// For each order, if the meta contains a `RaindexSignedContextOracleV1`
/// entry, the oracle URL is extracted and every IO pair for the same exact URL
/// is fetched in one batch POST. Different URL batches execute concurrently.
/// Any additional entries produced by `injector` are appended after the oracle
/// entries (composition order: `[oracle..., injected...]`), and the composed
/// list is attached to the `QuoteV3.signedContext` before the multicall is
/// issued. This matters for gated orders whose `calculate-io` asserts on
/// signed context during quoting.
pub async fn get_order_quotes(
    orders: Vec<SgOrder>,
    block_number: Option<u64>,
    rpcs: Vec<String>,
    chunk_size: Option<usize>,
    counterparty: Address,
    injector: &dyn SignedContextInjector,
) -> Result<Vec<BatchOrderQuotesResponse>, Error> {
    get_order_quotes_inner(
        orders,
        None,
        block_number,
        rpcs,
        chunk_size,
        counterparty,
        injector,
    )
    .await
}

/// Quote only the requested IO pairs for each order.
pub async fn get_order_quotes_for_pairs(
    orders: Vec<SgOrder>,
    selected_pairs: &[Vec<Pair>],
    block_number: Option<u64>,
    rpcs: Vec<String>,
    chunk_size: Option<usize>,
    counterparty: Address,
    injector: &dyn SignedContextInjector,
) -> Result<Vec<BatchOrderQuotesResponse>, Error> {
    if selected_pairs.len() != orders.len() {
        return Err(Error::PairSelectionLengthMismatch {
            expected: orders.len(),
            actual: selected_pairs.len(),
        });
    }
    get_order_quotes_inner(
        orders,
        Some(selected_pairs),
        block_number,
        rpcs,
        chunk_size,
        counterparty,
        injector,
    )
    .await
}

async fn get_order_quotes_inner(
    orders: Vec<SgOrder>,
    selected_pairs: Option<&[Vec<Pair>]>,
    block_number: Option<u64>,
    rpcs: Vec<String>,
    chunk_size: Option<usize>,
    counterparty: Address,
    injector: &dyn SignedContextInjector,
) -> Result<Vec<BatchOrderQuotesResponse>, Error> {
    let started_at = QuoteTiming::now();
    info!(
        order_count = orders.len(),
        rpc_url_count = rpcs.len(),
        block_number = ?block_number,
        chunk_size = ?chunk_size,
        counterparty = %counterparty,
        "starting quote pair sweep"
    );

    let block_started_at = QuoteTiming::now();
    let req_block_number = match block_number {
        Some(block) => block,
        None => {
            #[cfg(target_family = "wasm")]
            {
                let urls = rpcs
                    .iter()
                    .map(|rpc| rpc.parse())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(Error::UrlParseError)?;
                let provider = mk_read_provider(&urls).map_err(Error::ReadProviderError)?;
                provider
                    .get_block_number()
                    .await
                    .map_err(|error| Error::TransportError(error.to_string()))?
            }
            #[cfg(not(target_family = "wasm"))]
            {
                resolve_latest_block_number(&rpcs).await?
            }
        }
    };
    info!(
        requested_block_number = ?block_number,
        resolved_block_number = req_block_number,
        duration_ms = block_started_at.elapsed_ms(),
        "resolved quote block number"
    );

    // Pair metadata is prepared in strict iteration order. Oracle pairs are
    // grouped by exact endpoint URL so each endpoint receives one batch POST.
    // Different endpoint batches execute concurrently below, then their
    // results are scattered back into pair order before the batched chain
    // quote.
    let mut pair_preparations: Vec<QuotePairPreparation> = Vec::new();
    let mut oracle_batches: Vec<OracleBatchPreparation> = Vec::new();
    let mut oracle_batch_indices: HashMap<String, usize> = HashMap::new();
    let mut skipped_self_trade_pair_count = 0usize;

    let target_build_started_at = QuoteTiming::now();
    for (order_index, order) in orders.iter().enumerate() {
        let order_struct: OrderV4 = order.clone().try_into()?;
        let raindex = Address::from_str(&order.raindex.id.0)?;
        let oracle_url = crate::oracle::extract_oracle_url(order);

        for (input_index, input) in order_struct.validInputs.iter().enumerate() {
            for (output_index, output) in order_struct.validOutputs.iter().enumerate() {
                if input.token == output.token {
                    skipped_self_trade_pair_count += 1;
                    continue;
                }
                if !pair_is_selected(
                    selected_pairs,
                    order_index,
                    input_index as u32,
                    output_index as u32,
                ) {
                    continue;
                }

                let pair_name = format!(
                    "{}/{}",
                    order
                        .inputs
                        .iter()
                        .find_map(|v| {
                            Address::from_str(&v.token.address.0).ok().and_then(|add| {
                                add.eq(&input.token).then_some(
                                    v.token.symbol.clone().unwrap_or("UNKNOWN".to_string()),
                                )
                            })
                        })
                        .unwrap_or("UNKNOWN".to_string()),
                    order
                        .outputs
                        .iter()
                        .find_map(|v| {
                            Address::from_str(&v.token.address.0).ok().and_then(|add| {
                                add.eq(&output.token).then_some(
                                    v.token.symbol.clone().unwrap_or("UNKNOWN".to_string()),
                                )
                            })
                        })
                        .unwrap_or("UNKNOWN".to_string())
                );

                let pair_preparation_index = pair_preparations.len();
                pair_preparations.push(QuotePairPreparation {
                    pair: Pair {
                        pair_name,
                        input_index: input_index as u32,
                        output_index: output_index as u32,
                    },
                    quote_target: QuoteTarget {
                        raindex,
                        quote_config: QuoteV3 {
                            order: order_struct.clone(),
                            inputIOIndex: U256::from(input_index),
                            outputIOIndex: U256::from(output_index),
                            signedContext: vec![],
                        },
                    },
                    oracle_url: oracle_url.clone(),
                });

                // Oracle context remains live even when the quote is pinned to
                // a historical block. All pairs sharing the exact configured
                // endpoint URL are encoded into one request body below.
                if let Some(url) = &oracle_url {
                    let batch_index = match oracle_batch_indices.get(url) {
                        Some(index) => *index,
                        None => {
                            let index = oracle_batches.len();
                            oracle_batches.push(OracleBatchPreparation {
                                url: url.clone(),
                                items: Vec::new(),
                            });
                            oracle_batch_indices.insert(url.clone(), index);
                            index
                        }
                    };
                    oracle_batches[batch_index].items.push(OracleBatchItem {
                        pair_preparation_index,
                    });
                }
            }
        }
    }

    let oracle_fetch_count: usize = oracle_batches.iter().map(|batch| batch.items.len()).sum();
    let oracle_batch_request_count = oracle_batches.len();
    let oracle_batch_requests: Vec<OracleBatchRequest> = oracle_batches
        .iter()
        .map(|batch| {
            let body = crate::oracle::encode_oracle_body_batch(
                batch
                    .items
                    .iter()
                    .map(|item| {
                        let preparation = &pair_preparations[item.pair_preparation_index];
                        (
                            &preparation.quote_target.quote_config.order,
                            preparation.pair.input_index,
                            preparation.pair.output_index,
                            counterparty,
                        )
                    })
                    .collect(),
            );
            OracleBatchRequest::new(batch.url.clone(), body, batch.items.len())
        })
        .collect();
    let oracle_fetch_started_at = QuoteTiming::now();
    info!(
        oracle_request_count = oracle_fetch_count,
        oracle_endpoint_count = oracle_batch_request_count,
        oracle_concurrency_limit = ORACLE_REQUEST_CONCURRENCY_LIMIT,
        oracle_batch_request_count,
        "starting bounded batched quote oracle context fetches"
    );
    let oracle_batch_results: Vec<Result<Vec<Result<SignedContextV2, String>>, String>> =
        if oracle_batch_requests.is_empty() {
            vec![]
        } else {
            match OracleClient::new() {
                Ok(client) => client
                    .fetch_signed_context_batches(oracle_batch_requests)
                    .await
                    .into_iter()
                    .map(|result| {
                        result
                            .map(|items| {
                                items
                                    .into_iter()
                                    .map(|item| item.map_err(|error| error.to_string()))
                                    .collect()
                            })
                            .map_err(|error| error.to_string())
                    })
                    .collect(),
                Err(error) => {
                    let error = error.to_string();
                    (0..oracle_batch_request_count)
                        .map(|_| Err(error.clone()))
                        .collect()
                }
            }
        };
    let mut oracle_results: Vec<Option<Result<SignedContextV2, String>>> =
        std::iter::repeat_with(|| None)
            .take(pair_preparations.len())
            .collect();
    for (batch, batch_result) in oracle_batches.iter().zip(oracle_batch_results) {
        match batch_result {
            Ok(contexts) => {
                for (item, context) in batch.items.iter().zip(contexts) {
                    oracle_results[item.pair_preparation_index] = Some(context);
                }
            }
            Err(error) => {
                for item in &batch.items {
                    oracle_results[item.pair_preparation_index] = Some(Err(error.clone()));
                }
            }
        }
    }
    let oracle_fetch_failure_count = oracle_results
        .iter()
        .filter(|result| result.as_ref().is_some_and(Result::is_err))
        .count();
    info!(
        oracle_request_count = oracle_fetch_count,
        oracle_success_count = oracle_fetch_count.saturating_sub(oracle_fetch_failure_count),
        oracle_failure_count = oracle_fetch_failure_count,
        oracle_endpoint_count = oracle_batch_request_count,
        oracle_batch_request_count,
        oracle_concurrency_limit = ORACLE_REQUEST_CONCURRENCY_LIMIT,
        duration_ms = oracle_fetch_started_at.elapsed_ms(),
        "completed bounded batched quote oracle context fetches"
    );

    // Responses are assembled in strict iteration order. Pairs whose oracle
    // fetch failed get a failure response stored immediately; quoted pairs
    // leave a hole that is filled in after the RPC batch returns.
    let mut all_responses: Vec<Option<BatchOrderQuotesResponse>> = Vec::new();
    let mut all_quote_targets: Vec<QuoteTarget> = Vec::new();
    let mut quoted_pair_metadata: Vec<QuotedPairMetadata> = Vec::new();
    let mut injected_context_count = 0usize;

    for (preparation, oracle_result) in pair_preparations.into_iter().zip(oracle_results) {
        let oracle_context = if preparation.oracle_url.is_some() {
            oracle_result
                .unwrap_or_else(|| Err("Missing oracle response slot".to_string()))
                .map(|context| vec![context])
        } else {
            Ok(vec![])
        };
        let input_index = preparation.pair.input_index;
        let output_index = preparation.pair.output_index;
        let raindex = preparation.quote_target.raindex;
        let slot_idx = all_responses.len();

        match oracle_context {
            Ok(oracle_context) => {
                if preparation.oracle_url.is_some() {
                    debug!(
                        raindex = %raindex,
                        input_index,
                        output_index,
                        "fetched quote oracle context"
                    );
                }

                let injected = injector
                    .contexts_for(
                        &preparation.quote_target.quote_config.order,
                        input_index,
                        output_index,
                        counterparty,
                    )
                    .await;
                injected_context_count += injected.len();
                let composed: Vec<SignedContextV2> =
                    oracle_context.into_iter().chain(injected).collect();
                let mut quote_target = preparation.quote_target;
                quote_target.quote_config.signedContext = composed;

                all_responses.push(None);
                quoted_pair_metadata.push(QuotedPairMetadata {
                    slot_index: slot_idx,
                    pair: preparation.pair,
                });
                all_quote_targets.push(quote_target);
            }
            Err(error) => {
                warn!(
                    raindex = %raindex,
                    input_index,
                    output_index,
                    error = %error,
                    "quote oracle context fetch failed"
                );
                all_responses.push(Some(BatchOrderQuotesResponse {
                    pair: preparation.pair,
                    block_number: req_block_number,
                    success: false,
                    data: None,
                    error: Some(format!(
                        "Oracle fetch failed for pair ({input_index}, {output_index}): {error}"
                    )),
                    signed_context: vec![],
                }));
            }
        }
    }
    info!(
        order_count = orders.len(),
        target_count = all_quote_targets.len(),
        response_slot_count = all_responses.len(),
        skipped_self_trade_pair_count,
        oracle_fetch_count,
        oracle_fetch_failure_count,
        injected_context_count,
        duration_ms = target_build_started_at.elapsed_ms(),
        "built quote targets"
    );

    let batch_quote_started_at = QuoteTiming::now();
    let batch_quote_target = BatchQuoteTarget(all_quote_targets);
    let quote_result = batch_quote_target
        .do_quote(rpcs, Some(req_block_number), counterparty, chunk_size)
        .await;
    let signed_contexts = batch_quote_target
        .0
        .into_iter()
        .map(|target| target.quote_config.signedContext);
    let quote_results: Vec<(usize, BatchOrderQuotesResponse)> = match quote_result {
        Ok(quote_values) => {
            let failed_quote_count = quote_values.iter().filter(|result| result.is_err()).count();
            let successful_quote_count = quote_values.len().saturating_sub(failed_quote_count);
            info!(
                successful_quote_count,
                failed_quote_count,
                duration_ms = batch_quote_started_at.elapsed_ms(),
                "completed quote batch RPC"
            );
            quote_values
                .into_iter()
                .zip(quoted_pair_metadata.into_iter().zip(signed_contexts))
                .map(|(quote_result, (metadata, signed_context))| {
                    let response = match quote_result {
                        Ok(data) => BatchOrderQuotesResponse {
                            pair: metadata.pair,
                            block_number: req_block_number,
                            success: true,
                            data: Some(data),
                            error: None,
                            signed_context,
                        },
                        Err(e) => BatchOrderQuotesResponse {
                            pair: metadata.pair,
                            block_number: req_block_number,
                            success: false,
                            data: None,
                            error: Some(e.to_string()),
                            signed_context,
                        },
                    };
                    (metadata.slot_index, response)
                })
                .collect()
        }
        Err(e) => {
            let error = e.to_string();
            warn!(
                quoted_pair_count = quoted_pair_metadata.len(),
                duration_ms = batch_quote_started_at.elapsed_ms(),
                error = %error,
                "quote batch RPC failed"
            );
            quoted_pair_metadata
                .into_iter()
                .zip(signed_contexts)
                .map(|(metadata, signed_context)| {
                    let response = BatchOrderQuotesResponse {
                        pair: metadata.pair,
                        block_number: req_block_number,
                        success: false,
                        data: None,
                        error: Some(error.clone()),
                        signed_context,
                    };
                    (metadata.slot_index, response)
                })
                .collect()
        }
    };

    // Scatter quote results back into the iteration-ordered response vector.
    for (slot_idx, response) in quote_results {
        all_responses[slot_idx] = Some(response);
    }

    let responses: Vec<_> = all_responses.into_iter().map(|r| r.unwrap()).collect();
    let successful_quote_count = responses.iter().filter(|response| response.success).count();
    let failed_quote_count = responses.len().saturating_sub(successful_quote_count);
    info!(
        order_count = orders.len(),
        response_count = responses.len(),
        successful_quote_count,
        failed_quote_count,
        resolved_block_number = req_block_number,
        duration_ms = started_at.elapsed_ms(),
        "completed quote pair sweep"
    );

    Ok(responses)
}

#[cfg(not(target_family = "wasm"))]
async fn resolve_latest_block_number(rpcs: &[String]) -> Result<u64, Error> {
    let mut failures = Vec::new();
    for rpc in rpcs {
        let url = match rpc.parse() {
            Ok(url) => url,
            Err(error) => {
                failures.push(format!("{rpc}: {error}"));
                continue;
            }
        };
        let provider = match mk_read_provider(&[url]) {
            Ok(provider) => provider,
            Err(error) => {
                failures.push(format!("{rpc}: {error}"));
                continue;
            }
        };
        match with_rpc_timeout(provider.get_block_number(), RPC_ATTEMPT_TIMEOUT_MS).await {
            Some(Ok(value)) => return Ok(value),
            Some(Err(error)) => failures.push(format!("{rpc}: {error}")),
            None => failures.push(format!("{rpc}: timed out after {RPC_ATTEMPT_TIMEOUT_MS}ms")),
        }
    }
    Err(Error::TransportError(format!(
        "all quote RPC block reads failed: {}",
        failures.join("; ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_selection_is_scoped_by_order_and_io_indices() {
        let selections = vec![
            vec![Pair {
                pair_name: String::new(),
                input_index: 1,
                output_index: 2,
            }],
            Vec::new(),
        ];

        assert!(pair_is_selected(Some(&selections), 0, 1, 2));
        assert!(!pair_is_selected(Some(&selections), 0, 0, 2));
        assert!(!pair_is_selected(Some(&selections), 1, 1, 2));
        assert!(pair_is_selected(None, 1, 1, 2));
    }

    #[tokio::test]
    async fn pair_selection_must_align_with_orders() {
        let error = get_order_quotes_for_pairs(
            Vec::new(),
            &[Vec::new()],
            None,
            Vec::new(),
            None,
            Address::ZERO,
            &NoopInjector,
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            Error::PairSelectionLengthMismatch {
                expected: 0,
                actual: 1
            }
        ));
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn latest_block_read_cycles_after_malformed_rpc_url() {
        let server = MockServer::start_async().await;
        let block = server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x2a",
            }));
        });

        let result = resolve_latest_block_number(&["not a URL".into(), server.url("/rpc")])
            .await
            .unwrap();

        assert_eq!(result, 42);
        block.assert_hits(1);
    }

    #[cfg(not(target_family = "wasm"))]
    use alloy::primitives::{address, Bytes};
    use alloy::{
        hex::{encode_prefixed, FromHexError},
        primitives::B256,
        providers::Provider,
        sol_types::{SolCall, SolValue},
    };
    #[cfg(not(target_family = "wasm"))]
    use httpmock::{Method::POST, MockServer};
    use rain_math_float::Float;
    #[cfg(not(target_family = "wasm"))]
    use rain_metadata::types::raindex_signed_context_oracle::RaindexSignedContextOracleV1;
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_common::{add_order::AddOrderArgs, dotrain_order::DotrainOrder};
    use raindex_subgraph_client::types::{
        common::{SgBigInt, SgBytes, SgErc20, SgRaindex, SgVault},
        order_detail_traits::OrderDetailError,
    };
    use raindex_subgraph_client::utils::float::*;
    use raindex_test_fixtures::LocalEvm;
    #[cfg(not(target_family = "wasm"))]
    use serde_json::json;
    #[cfg(not(target_family = "wasm"))]
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    #[cfg(not(target_family = "wasm"))]
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    #[cfg(not(target_family = "wasm"))]
    struct TestInjector {
        context: SignedContextV2,
        call_count: Arc<AtomicUsize>,
    }

    #[cfg(not(target_family = "wasm"))]
    #[async_trait::async_trait]
    impl SignedContextInjector for TestInjector {
        async fn contexts_for(
            &self,
            _order: &OrderV4,
            _input_io_index: u32,
            _output_io_index: u32,
            _counterparty: Address,
        ) -> Vec<SignedContextV2> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            vec![self.context.clone()]
        }
    }

    #[cfg(not(target_family = "wasm"))]
    async fn read_http_body(stream: &mut TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let (body_start, content_length) = loop {
            let mut chunk = [0u8; 4096];
            let bytes_read = stream.read(&mut chunk).await.unwrap();
            assert!(bytes_read > 0, "request ended before HTTP headers");
            request.extend_from_slice(&chunk[..bytes_read]);

            if let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let body_start = header_end + 4;
                let headers = std::str::from_utf8(&request[..header_end]).unwrap();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                break (body_start, content_length);
            }
        };

        while request.len() < body_start + content_length {
            let mut chunk = [0u8; 4096];
            let bytes_read = stream.read(&mut chunk).await.unwrap();
            assert!(bytes_read > 0, "request ended before HTTP body");
            request.extend_from_slice(&chunk[..bytes_read]);
        }

        request[body_start..body_start + content_length].to_vec()
    }

    #[cfg(not(target_family = "wasm"))]
    async fn start_batch_oracle_server(
        responses: Vec<Result<crate::oracle::OracleResponse, crate::oracle::OracleBatchItemError>>,
    ) -> (String, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/oracle", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let body = read_http_body(&mut stream).await;
            let responses = responses
                .into_iter()
                .map(|response| match response {
                    Ok(body) => json!({"status": "ok", "body": body}),
                    Err(body) => json!({"status": "error", "body": body}),
                })
                .collect::<Vec<_>>();
            let payload = serde_json::to_vec(&responses).unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(&payload).await.unwrap();
            vec![body]
        });
        (url, server)
    }

    #[cfg(not(target_family = "wasm"))]
    async fn start_quote_rpc_server(
        expected_calls: Vec<Bytes>,
        result: String,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/rpc", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let body = read_http_body(&mut stream).await;
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let transaction = &request["params"][0];
            let calldata = transaction
                .get("input")
                .or_else(|| transaction.get("data"))
                .and_then(serde_json::Value::as_str)
                .unwrap();
            let calldata = alloy::hex::decode(calldata).unwrap();
            let multicall =
                raindex_bindings::Raindex::multicallCall::abi_decode(&calldata).unwrap();
            assert_eq!(multicall.data, expected_calls);

            let payload = serde_json::to_vec(&json!({
                "jsonrpc": "2.0",
                "id": request["id"].clone(),
                "result": result,
            }))
            .unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(&payload).await.unwrap();
        });
        (url, server)
    }

    struct TestSetup {
        local_evm: LocalEvm,
        owner: Address,
        token1: SgErc20,
        token2: SgErc20,
        raindex: Address,
    }

    async fn setup_test() -> TestSetup {
        let mut local_evm = LocalEvm::new().await;
        let owner = local_evm.signer_wallets[0].default_signer().address();

        let token1 = local_evm
            .deploy_new_token("Token1", "Token1", 18, U256::MAX, owner)
            .await;
        let token2 = local_evm
            .deploy_new_token("Token2", "Token2", 18, U256::MAX, owner)
            .await;
        let raindex = *local_evm.raindex.address();

        TestSetup {
            local_evm,
            owner,
            token1: SgErc20 {
                id: SgBytes(token1.address().to_string()),
                address: SgBytes(token1.address().to_string()),
                name: Some("Token1".to_string()),
                symbol: Some("Token1".to_string()),
                decimals: Some(SgBigInt(18.to_string())),
            },
            token2: SgErc20 {
                id: SgBytes(token2.address().to_string()),
                address: SgBytes(token2.address().to_string()),
                name: Some("Token2".to_string()),
                symbol: Some("Token2".to_string()),
                decimals: Some(SgBigInt(18.to_string())),
            },
            raindex,
        }
    }

    fn create_dotrain_config(setup: &TestSetup) -> String {
        format!(
            r#"
version: {spec_version}
networks:
    some-key:
        rpcs:
            - {rpc_url}
        chain-id: 123
        network-id: 123
        currency: ETH
rainlangs:
    some-key:
        address: {rainlang_address}
tokens:
    t2:
        network: some-key
        address: {token2}
        decimals: 18
        label: Token2
        symbol: Token2
    t1:
        network: some-key
        address: {token1}
        decimals: 18
        label: Token1
        symbol: token1
raindex:
    some-key:
        address: {raindex}
orders:
    some-key:
        inputs:
            - token: t1
            - token: t2
        outputs:
            - token: t1
              vault-id: 0x01
            - token: t2
              vault-id: 0x01
scenarios:
    some-key:
        rainlang: some-key
        bindings:
            key1: 10
deployments:
    some-key:
        scenario: some-key
        order: some-key
---
#key1 !Test binding
#calculate-io
amount price: 2 3;
#handle-add-order
:;
#handle-io
:;
"#,
            rpc_url = setup.local_evm.url(),
            raindex = setup.raindex,
            rainlang_address = setup.local_evm.rainlang,
            token1 = setup.token1.address.0,
            token2 = setup.token2.address.0,
            spec_version = SpecVersion::current(),
        )
    }

    async fn create_order(setup: &TestSetup, dotrain: String) -> String {
        let dotrain_order = DotrainOrder::create(dotrain.clone(), None).await.unwrap();
        let deployment = dotrain_order
            .dotrain_yaml()
            .get_deployment("some-key")
            .unwrap();
        let calldata = AddOrderArgs::new_from_deployment(dotrain, deployment, None)
            .await
            .unwrap()
            .try_into_call(vec![setup.local_evm.url()])
            .await
            .unwrap()
            .abi_encode();

        encode_prefixed(
            setup
                .local_evm
                .add_order(&calldata, setup.owner)
                .await
                .0
                .order
                .abi_encode(),
        )
    }

    fn create_vault(vault_id: B256, setup: &TestSetup, token: &SgErc20) -> SgVault {
        SgVault {
            id: SgBytes(vault_id.to_string()),
            token: token.clone(),
            balance: SgBytes(F6.as_hex()),
            vault_id: SgBytes(vault_id.to_string()),
            owner: SgBytes(setup.local_evm.anvil.addresses()[0].to_string()),
            raindex: SgRaindex {
                id: SgBytes(setup.raindex.to_string()),
            },
            orders_as_input: vec![],
            orders_as_output: vec![],
            balance_changes: vec![],
        }
    }

    fn create_sg_order(
        setup: &TestSetup,
        order_bytes: String,
        inputs: Vec<SgVault>,
        outputs: Vec<SgVault>,
    ) -> SgOrder {
        SgOrder {
            id: SgBytes(B256::random().to_string()),
            raindex: SgRaindex {
                id: SgBytes(setup.raindex.to_string()),
            },
            order_bytes: SgBytes(order_bytes),
            order_hash: SgBytes(B256::random().to_string()),
            owner: SgBytes(setup.local_evm.anvil.addresses()[0].to_string()),
            outputs,
            inputs,
            active: true,
            add_events: vec![],
            meta: None,
            timestamp_added: SgBigInt(0.to_string()),
            trades: vec![],
            remove_events: vec![],
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn with_oracle_url(mut order: SgOrder, oracle_url: &str) -> SgOrder {
        let oracle = RaindexSignedContextOracleV1::parse(oracle_url).unwrap();
        order.meta = Some(SgBytes(encode_prefixed(oracle.cbor_encode().unwrap())));
        order
    }

    #[tokio::test]
    async fn test_get_order_quotes_ok() {
        let setup = setup_test().await;

        let vault_id_const = B256::from(U256::from(1u64));
        let vault_id1 = vault_id_const; // for token1
        let vault_id2 = vault_id_const; // for token2

        // Deposit in token1 and token2 vaults
        setup
            .local_evm
            .deposit(
                setup.owner,
                Address::from_str(&setup.token1.address.0).unwrap(),
                U256::from(10).pow(U256::from(66)),
                18,
                vault_id1,
            )
            .await;
        setup
            .local_evm
            .deposit(
                setup.owner,
                Address::from_str(&setup.token2.address.0).unwrap(),
                U256::from(10).pow(U256::from(66)),
                18,
                vault_id2,
            )
            .await;

        let dotrain = create_dotrain_config(&setup);
        let order = create_order(&setup, dotrain).await;

        let vault1 = create_vault(vault_id1, &setup, &setup.token1);
        let vault2 = create_vault(vault_id2, &setup, &setup.token2);

        // does not follow the actual original order's io order
        let inputs = vec![vault2.clone(), vault1.clone()];
        let outputs = vec![vault2.clone(), vault1.clone()];

        let order = create_sg_order(&setup, order, inputs, outputs);

        let result = get_order_quotes(
            vec![order],
            None,
            vec![setup.local_evm.url()],
            None,
            Address::ZERO,
            &NoopInjector,
        )
        .await
        .unwrap();

        let expected_max_output = Float::parse("2".to_string()).unwrap();
        let expected_ratio = Float::parse("3".to_string()).unwrap();

        let block_number = setup.local_evm.provider.get_block_number().await.unwrap();
        let expected = [
            BatchOrderQuotesResponse {
                pair: Pair {
                    pair_name: "Token1/Token2".to_string(),
                    input_index: 0,
                    output_index: 1,
                },
                block_number,
                data: Some(OrderQuoteValue {
                    max_output: expected_max_output,
                    ratio: expected_ratio,
                }),
                success: true,
                error: None,
                signed_context: vec![],
            },
            BatchOrderQuotesResponse {
                pair: Pair {
                    pair_name: "Token2/Token1".to_string(),
                    input_index: 1,
                    output_index: 0,
                },
                block_number,
                data: Some(OrderQuoteValue {
                    max_output: expected_max_output,
                    ratio: expected_ratio,
                }),
                success: true,
                error: None,
                signed_context: vec![],
            },
        ];

        assert_eq!(result.len(), expected.len());

        for (res, exp) in result.iter().zip(expected.iter()) {
            assert_eq!(res.pair, exp.pair);
            assert_eq!(res.block_number, exp.block_number);
            assert_eq!(res.success, exp.success);
            assert_eq!(res.error, exp.error);

            let actual_data = res.data.unwrap();
            let expected_data = exp.data.unwrap();

            assert!(
                actual_data.max_output.eq(expected_data.max_output).unwrap(),
                "actual_data.max_output: {}, expected_data.max_output: {}",
                actual_data.max_output.format().unwrap(),
                expected_data.max_output.format().unwrap()
            );
            assert!(
                actual_data.ratio.eq(expected_data.ratio).unwrap(),
                "actual_data.ratio: {}, expected_data.ratio: {}",
                actual_data.ratio.format().unwrap(),
                expected_data.ratio.format().unwrap()
            );
        }
    }

    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn test_get_order_quotes_batches_oracle_results_and_preserves_alignment() {
        let setup = setup_test().await;
        let vault_id = B256::from(U256::from(1u64));
        setup
            .local_evm
            .deposit(
                setup.owner,
                Address::from_str(&setup.token1.address.0).unwrap(),
                U256::from(10).pow(U256::from(66)),
                18,
                vault_id,
            )
            .await;
        setup
            .local_evm
            .deposit(
                setup.owner,
                Address::from_str(&setup.token2.address.0).unwrap(),
                U256::from(10).pow(U256::from(66)),
                18,
                vault_id,
            )
            .await;

        let order_bytes = create_order(&setup, create_dotrain_config(&setup)).await;
        let vault1 = create_vault(vault_id, &setup, &setup.token1);
        let vault2 = create_vault(vault_id, &setup, &setup.token2);
        let inputs = vec![vault2.clone(), vault1.clone()];
        let outputs = vec![vault2, vault1];
        let oracle_contexts = [
            SignedContextV2 {
                signer: address!("0x1111111111111111111111111111111111111111"),
                domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
                context: vec![B256::with_last_byte(1)],
                signature: vec![0xaa].into(),
            },
            SignedContextV2 {
                signer: address!("0x3333333333333333333333333333333333333333"),
                domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
                context: vec![B256::with_last_byte(3)],
                signature: vec![0xcc].into(),
            },
        ];
        let oracle_responses = oracle_contexts
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, context)| {
                if index == 0 {
                    Ok(crate::oracle::OracleResponse {
                        signer: context.signer,
                        domain_separator: context.domainSeparator,
                        context: context.context,
                        signature: context.signature,
                    })
                } else {
                    Err(crate::oracle::OracleBatchItemError {
                        error: "service_unavailable".to_string(),
                        detail: "pair unavailable".to_string(),
                    })
                }
            })
            .collect();
        let base_oracle_order =
            create_sg_order(&setup, order_bytes.clone(), inputs.clone(), outputs.clone());
        let order_struct: OrderV4 = base_oracle_order.clone().try_into().unwrap();
        let expected_oracle_body = crate::oracle::encode_oracle_body_batch(vec![
            (&order_struct, 0, 1, Address::ZERO),
            (&order_struct, 1, 0, Address::ZERO),
        ]);
        let (oracle_url, oracle_server) = start_batch_oracle_server(oracle_responses).await;
        let oracle_order = with_oracle_url(base_oracle_order, &oracle_url);
        let plain_order = create_sg_order(&setup, order_bytes, inputs, outputs);
        let injected_context = SignedContextV2 {
            signer: address!("0x2222222222222222222222222222222222222222"),
            domainSeparator: alloy::primitives::B256::repeat_byte(0xd5),
            context: vec![B256::with_last_byte(2)],
            signature: vec![0xbb].into(),
        };
        let injector_call_count = Arc::new(AtomicUsize::new(0));
        let injector = TestInjector {
            context: injected_context.clone(),
            call_count: injector_call_count.clone(),
        };
        let quote_values = [("2", "3"), ("6", "7"), ("8", "9")].map(|(output_max, ratio)| {
            (
                Float::parse(output_max.to_string()).unwrap(),
                Float::parse(ratio.to_string()).unwrap(),
            )
        });
        let quote_results = quote_values
            .iter()
            .map(|(output_max, ratio)| {
                Bytes::from(
                    raindex_bindings::IRaindexV7::quote3Call::abi_encode_returns(
                        &raindex_bindings::IRaindexV7::quote3Return {
                            exists: true,
                            outputMax: output_max.get_inner(),
                            ioRatio: ratio.get_inner(),
                        },
                    ),
                )
            })
            .collect::<Vec<_>>();
        let expected_oracle_quote_call = Bytes::from(
            raindex_bindings::IRaindexV7::quote3Call {
                quoteConfig: QuoteV3 {
                    order: order_struct.clone(),
                    inputIOIndex: U256::ZERO,
                    outputIOIndex: U256::from(1),
                    signedContext: vec![oracle_contexts[0].clone(), injected_context.clone()],
                },
            }
            .abi_encode(),
        );
        let expected_plain_quote_call = Bytes::from(
            raindex_bindings::IRaindexV7::quote3Call {
                quoteConfig: QuoteV3 {
                    order: order_struct.clone(),
                    inputIOIndex: U256::ZERO,
                    outputIOIndex: U256::from(1),
                    signedContext: vec![injected_context.clone()],
                },
            }
            .abi_encode(),
        );
        let expected_second_plain_quote_call = Bytes::from(
            raindex_bindings::IRaindexV7::quote3Call {
                quoteConfig: QuoteV3 {
                    order: order_struct,
                    inputIOIndex: U256::from(1),
                    outputIOIndex: U256::ZERO,
                    signedContext: vec![injected_context.clone()],
                },
            }
            .abi_encode(),
        );
        let expected_rpc_calls = vec![
            expected_oracle_quote_call,
            expected_plain_quote_call,
            expected_second_plain_quote_call,
        ];
        let rpc_result = encode_prefixed(<Vec<Bytes> as SolValue>::abi_encode(&quote_results));
        let (rpc_url, rpc_server) = start_quote_rpc_server(expected_rpc_calls, rpc_result).await;

        let responses = get_order_quotes(
            vec![oracle_order, plain_order],
            Some(1),
            vec![rpc_url],
            None,
            Address::ZERO,
            &injector,
        )
        .await
        .unwrap();

        assert_eq!(responses.len(), 4);
        assert_eq!(
            responses
                .iter()
                .map(|response| response.success)
                .collect::<Vec<_>>(),
            vec![true, false, true, true]
        );
        assert_eq!(
            responses
                .iter()
                .map(|response| (response.pair.input_index, response.pair.output_index))
                .collect::<Vec<_>>(),
            vec![(0, 1), (1, 0), (0, 1), (1, 0)]
        );

        for (response_index, quote_value_index) in [(0, 0), (2, 1), (3, 2)] {
            let data = responses[response_index].data.as_ref().unwrap();
            let (expected_output_max, expected_ratio) = &quote_values[quote_value_index];
            assert!(data.max_output.eq(*expected_output_max).unwrap());
            assert!(data.ratio.eq(*expected_ratio).unwrap());
        }

        assert!(responses[1].data.is_none());
        assert!(responses[1]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("service_unavailable")));

        assert_eq!(responses[0].signed_context.len(), 2);
        assert_eq!(
            responses[0].signed_context[0].signer,
            oracle_contexts[0].signer
        );
        assert_eq!(
            responses[0].signed_context[1].signer,
            injected_context.signer
        );
        assert!(responses[1].signed_context.is_empty());
        for response in &responses[2..] {
            assert_eq!(response.signed_context.len(), 1);
            assert_eq!(response.signed_context[0].signer, injected_context.signer);
        }

        assert_eq!(injector_call_count.load(Ordering::SeqCst), 3);
        let oracle_request_bodies = oracle_server.await.unwrap();
        assert_eq!(oracle_request_bodies, vec![expected_oracle_body]);
        rpc_server.await.unwrap();
    }

    #[tokio::test]
    async fn test_get_order_quotes_err() {
        let setup = setup_test().await;
        let dotrain = create_dotrain_config(&setup);
        let order = create_order(&setup, dotrain).await;

        // Test invalid raindex address
        let mut invalid_order = create_sg_order(&setup, order.clone(), vec![], vec![]);
        invalid_order.raindex.id = SgBytes("invalid_address".to_string());

        let err = get_order_quotes(
            vec![invalid_order],
            None,
            vec![setup.local_evm.url()],
            None,
            Address::ZERO,
            &NoopInjector,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, Error::FromHexError(FromHexError::OddLength)));

        // Test invalid order bytes
        let invalid_order = create_sg_order(&setup, B256::random().to_string(), vec![], vec![]);

        let err = get_order_quotes(
            vec![invalid_order],
            None,
            vec![setup.local_evm.url()],
            None,
            Address::ZERO,
            &NoopInjector,
        )
        .await
        .unwrap_err();

        assert!(matches!(
            err,
            Error::OrderDetailError(OrderDetailError::AbiDecode(_))
        ));

        // Test invalid RPC URL
        let valid_order = create_sg_order(&setup, order, vec![], vec![]);

        let err = get_order_quotes(
            vec![valid_order],
            None,
            vec!["invalid_rpc_url".to_string()],
            None,
            Address::ZERO,
            &NoopInjector,
        )
        .await
        .unwrap_err();

        assert!(matches!(
            err,
            Error::TransportError(message)
                if message.contains("all quote RPC block reads failed")
        ));
    }
}
