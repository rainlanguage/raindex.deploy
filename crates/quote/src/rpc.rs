use crate::{
    error::{Error, FailedQuote},
    quote::{QuoteResult, QuoteTarget},
};
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::serde::WithOtherFields;
use alloy::sol_types::SolCall;
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    transports::RpcError,
};
use futures::future::join_all;
use rain_error_decoding::{AbiDecodedErrorType, ErrorRegistry};
use raindex_bindings::provider::{mk_read_provider, ReadProvider};
use raindex_bindings::IRaindexV7::quote3Call;
use raindex_bindings::Raindex::multicallCall;
#[cfg(not(target_family = "wasm"))]
use std::future::Future;
use url::Url;

const DEFAULT_QUOTE_CHUNK_SIZE: usize = 16;
#[cfg(not(target_family = "wasm"))]
pub(crate) const RPC_ATTEMPT_TIMEOUT_MS: u64 = 3_000;

#[cfg(not(target_family = "wasm"))]
pub(crate) async fn with_rpc_timeout<F>(future: F, timeout_ms: u64) -> Option<F::Output>
where
    F: Future,
{
    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), future)
        .await
        .ok()
}

fn normalize_chunk_size(chunk_size: Option<usize>) -> usize {
    chunk_size.unwrap_or(DEFAULT_QUOTE_CHUNK_SIZE).max(1)
}

/// Classify a chunk-level failure into per-target `QuoteResult`s when the
/// bisection has narrowed down to a single target (or cannot narrow further).
fn single_quote_failure(err: &Error) -> QuoteResult {
    Err(FailedQuote::CorruptReturnData(format!(
        "Single quote failed after chunk split: {err}"
    )))
}

/// Issue a single `raindex.multicall([quote3, quote3, ...])` RPC call with
/// `from = counterparty` so each inner `quote3` sees `msg.sender = counterparty`.
///
/// RaindexV7 inherits OpenZeppelin's `Multicall` which `delegatecall`s each
/// element back into the raindex itself. `delegatecall` preserves the outer
/// `msg.sender`, so the calldata built here gives API-gated strategies (which
/// read `order-counterparty()` in `calculate-io`) the correct taker address.
///
/// All `quote_targets` passed here MUST share the same `raindex` address —
/// that grouping is the caller's responsibility (see `batch_quote`).
///
/// Behavior:
/// - Success path: returns a `Vec<QuoteResult>` aligned 1:1 with `quote_targets`.
///   Per-target `exists == false` becomes `FailedQuote::NonExistent`.
/// - Revert path: OZ `Multicall` bubbles the FIRST reverting inner call's
///   revert data. There is no per-element error isolation, so this function
///   returns `Err(Error::...)` with a decoded `FailedQuote` surface. The
///   caller (`quote_chunk_with_probe_and_split`) is responsible for bisecting
///   to attribute the revert to a specific target.
async fn quote_chunk_once(
    quote_targets: &[QuoteTarget],
    provider: &ReadProvider,
    block_number: Option<u64>,
    counterparty: Address,
    registry: Option<&dyn ErrorRegistry>,
) -> Result<Vec<QuoteResult>, Error> {
    debug_assert!(!quote_targets.is_empty());
    let raindex = quote_targets[0].raindex;
    debug_assert!(quote_targets.iter().all(|t| t.raindex == raindex));

    let inner_calls: Vec<Bytes> = quote_targets
        .iter()
        .map(|t| {
            Bytes::from(
                quote3Call {
                    quoteConfig: t.quote_config.clone(),
                }
                .abi_encode(),
            )
        })
        .collect();

    let calldata = multicallCall {
        data: inner_calls.clone(),
    }
    .abi_encode();

    let tx = TransactionRequest::default()
        .with_to(raindex)
        .with_from(counterparty)
        .with_input(calldata);
    let tx = WithOtherFields::new(tx);

    let block = block_number
        .map(|n| BlockId::Number(BlockNumberOrTag::Number(n)))
        .unwrap_or(BlockId::latest());

    #[cfg(not(target_family = "wasm"))]
    let response = with_rpc_timeout(
        async { provider.call(tx).block(block).await },
        RPC_ATTEMPT_TIMEOUT_MS,
    )
    .await
    .ok_or_else(|| {
        Error::TransportError(format!(
            "quote request timed out after {RPC_ATTEMPT_TIMEOUT_MS}ms"
        ))
    })?;
    #[cfg(target_family = "wasm")]
    let response = provider.call(tx).block(block).await;

    match response {
        Ok(bytes) => {
            let decoded = multicallCall::abi_decode_returns(&bytes).map_err(|e| {
                Error::AlloySolTypesError(alloy::sol_types::Error::Other(
                    format!("failed to decode multicall results: {e}").into(),
                ))
            })?;

            if decoded.len() != quote_targets.len() {
                return Err(Error::AlloySolTypesError(alloy::sol_types::Error::Other(
                    format!(
                        "multicall length mismatch: expected {}, got {}",
                        quote_targets.len(),
                        decoded.len()
                    )
                    .into(),
                )));
            }

            let mut results: Vec<QuoteResult> = Vec::with_capacity(decoded.len());
            for element in decoded {
                match quote3Call::abi_decode_returns(&element) {
                    Ok(ret) => {
                        if ret.exists {
                            results.push(Ok(ret.into()));
                        } else {
                            results.push(Err(FailedQuote::NonExistent));
                        }
                    }
                    Err(e) => results.push(Err(FailedQuote::CorruptReturnData(format!(
                        "Failed to decode quote3 return: {e}"
                    )))),
                }
            }
            Ok(results)
        }
        Err(RpcError::ErrorResp(err_resp)) => {
            if let Some(revert) = err_resp.as_revert_data() {
                let decoded = match AbiDecodedErrorType::selector_registry_abi_decode(
                    revert.as_ref(),
                    registry,
                )
                .await
                {
                    Ok(abi_err) => FailedQuote::RevertError(abi_err),
                    Err(err) => FailedQuote::RevertErrorDecodeFailed(err),
                };
                // Wrap the per-target failure as a chunk-level error so the
                // bisection logic can decide whether to split or to attribute
                // it to a single target.
                Err(Error::ChunkReverted(Box::new(decoded)))
            } else {
                Err(Error::TransportError(format!(
                    "RPC error without revert data: {err_resp}"
                )))
            }
        }
        Err(e) => Err(Error::TransportError(format!("{e}"))),
    }
}

/// Execute `quote_chunk_once` for a single-raindex group. If it fails at the
/// chunk level (OZ Multicall bubbles the first revert), recursively bisect the
/// group to isolate which target is responsible. All targets in `quote_targets`
/// MUST share the same raindex.
async fn quote_chunk_with_probe_and_split(
    quote_targets: &[QuoteTarget],
    provider: &ReadProvider,
    block_number: Option<u64>,
    counterparty: Address,
    registry: Option<&dyn ErrorRegistry>,
) -> Result<Vec<QuoteResult>, Error> {
    let initial_err = match quote_chunk_once(
        quote_targets,
        provider,
        block_number,
        counterparty,
        registry,
    )
    .await
    {
        Ok(results) => return Ok(results),
        Err(err) => err,
    };

    // Retrying a transport failure with progressively smaller chunks
    // multiplies the same unhealthy RPC request and can turn a single rate
    // limit into a request storm. Return it so the caller can cycle RPCs.
    if matches!(initial_err, Error::TransportError(_)) {
        return Err(initial_err);
    }

    if quote_targets.len() <= 1 {
        // A singleton on-chain revert belongs to that target. Decode and
        // return it without trying another RPC, because the order itself is
        // invalid. Provider-level response errors can still be specific to an
        // unhealthy RPC, so let the caller cycle to the next provider.
        return if matches!(initial_err, Error::ChunkReverted(_)) {
            Ok(vec![chunk_err_to_quote_result(&initial_err)])
        } else {
            Err(initial_err)
        };
    }

    // Only an on-chain revert from the Raindex multicall can be attributed by
    // splitting a multi-target chunk.
    if !matches!(initial_err, Error::ChunkReverted(_)) {
        return Err(initial_err);
    }

    // Bisect unconditionally. A previous version short-circuited here with a
    // pair of "probe" singletons, on the theory that if both probes reverted
    // the whole chunk probably reverts uniformly. That optimisation was wrong
    // for OZ multicall: OZ bubbles the *first* reverting delegatecall, so a
    // chunk-level revert tells us nothing about targets beyond the offending
    // one. With heterogeneous chunks (e.g. a mix of stale-price and fresh
    // orders), the probes easily land on stale targets and every surviving
    // order in the chunk gets silently dropped. Always bisect; the RPC cost
    // of isolating a single reverting target is bounded by log2(chunk_size).

    let mut resolved: Vec<Option<QuoteResult>> = Vec::with_capacity(quote_targets.len());
    resolved.resize_with(quote_targets.len(), || None);
    let mut pending = vec![(0usize, quote_targets.len())];

    while let Some((start, end)) = pending.pop() {
        let chunk = &quote_targets[start..end];
        match quote_chunk_once(chunk, provider, block_number, counterparty, registry).await {
            Ok(results) => {
                for (offset, result) in results.into_iter().enumerate() {
                    resolved[start + offset] = Some(result);
                }
            }
            Err(err) => {
                if !matches!(err, Error::ChunkReverted(_)) {
                    return Err(err);
                } else if chunk.len() == 1 {
                    resolved[start] = Some(chunk_err_to_quote_result(&err));
                } else {
                    let mid = start + (chunk.len() / 2);
                    pending.push((mid, end));
                    pending.push((start, mid));
                }
            }
        }
    }

    Ok(resolved
        .into_iter()
        .map(|v| v.unwrap_or_else(|| single_quote_failure(&initial_err)))
        .collect())
}

/// Convert a chunk-level error produced by `quote_chunk_once` into a per-target
/// `QuoteResult`. This is how we attribute a bisected revert back to a single
/// target.
fn chunk_err_to_quote_result(err: &Error) -> QuoteResult {
    match err {
        Error::ChunkReverted(failed) => match failed.as_ref() {
            FailedQuote::RevertError(abi_err) => Err(FailedQuote::RevertError(abi_err.clone())),
            FailedQuote::CorruptReturnData(msg) => Err(FailedQuote::CorruptReturnData(msg.clone())),
            FailedQuote::RevertErrorDecodeFailed(_) => Err(FailedQuote::CorruptReturnData(
                "Revert could not be decoded by the selector registry".to_string(),
            )),
            FailedQuote::NonExistent => Err(FailedQuote::NonExistent),
            #[cfg(target_family = "wasm")]
            FailedQuote::SerdeWasmBindgenError(_) => Err(FailedQuote::CorruptReturnData(
                "wasm serde error surfaced as chunk revert".to_string(),
            )),
        },
        other => single_quote_failure(other),
    }
}

/// Quotes an array of `QuoteTarget`s.
///
/// Targets are partitioned by their `raindex` address so that each raindex
/// gets its own `multicall(bytes[])` RPC call. Using the raindex's own OZ
/// `Multicall` (rather than Multicall3) ensures `msg.sender` inside each
/// inner `quote3` is preserved as `counterparty` — critical for gated
/// strategies that read `order-counterparty()` inside `calculate-io`.
///
/// Within each raindex group, targets are chunked by `chunk_size` and each
/// chunk is quoted with probe-and-split bisection: on a chunk revert (OZ
/// Multicall bubbles the first failing inner call's revert), we recurse to
/// isolate which target is responsible.
///
/// The returned `Vec<QuoteResult>` is positionally aligned with the input
/// `quote_targets`.
pub async fn batch_quote(
    quote_targets: &[QuoteTarget],
    rpcs: Vec<String>,
    block_number: Option<u64>,
    counterparty: Address,
    registry: Option<&dyn ErrorRegistry>,
    chunk_size: Option<usize>,
) -> Result<Vec<QuoteResult>, Error> {
    if quote_targets.is_empty() {
        return Ok(vec![]);
    }
    #[cfg(target_family = "wasm")]
    {
        let rpcs = rpcs
            .into_iter()
            .map(|rpc| rpc.parse::<Url>())
            .collect::<Result<Vec<Url>, _>>()?;
        let provider = mk_read_provider(&rpcs)?;
        return batch_quote_with_provider(
            quote_targets,
            &provider,
            block_number,
            counterparty,
            registry,
            chunk_size,
        )
        .await;
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let mut transport_failures = Vec::new();
        let mut providers = Vec::new();
        for rpc in &rpcs {
            let url = match rpc.parse::<Url>() {
                Ok(url) => url,
                Err(error) => {
                    transport_failures.push(format!("{rpc}: {error}"));
                    continue;
                }
            };
            let provider = match mk_read_provider(std::slice::from_ref(&url)) {
                Ok(provider) => provider,
                Err(error) => {
                    transport_failures.push(format!("{rpc}: {error}"));
                    continue;
                }
            };
            providers.push((url, provider));
        }
        if providers.is_empty() {
            let reason = if rpcs.is_empty() {
                "no RPC URLs configured".to_string()
            } else {
                transport_failures.join("; ")
            };
            let message = format!("all quote RPCs failed: {reason}");
            return Ok((0..quote_targets.len())
                .map(|_| Err(FailedQuote::CorruptReturnData(message.clone())))
                .collect());
        }
        batch_quote_with_providers(
            quote_targets,
            &providers,
            transport_failures,
            block_number,
            counterparty,
            registry,
            chunk_size,
        )
        .await
    }
}

#[cfg(not(target_family = "wasm"))]
async fn batch_quote_with_providers(
    quote_targets: &[QuoteTarget],
    providers: &[(Url, ReadProvider)],
    provider_setup_failures: Vec<String>,
    block_number: Option<u64>,
    counterparty: Address,
    registry: Option<&dyn ErrorRegistry>,
    chunk_size: Option<usize>,
) -> Result<Vec<QuoteResult>, Error> {
    let chunk_size = normalize_chunk_size(chunk_size);
    let mut groups: Vec<(Address, Vec<(usize, QuoteTarget)>)> = Vec::new();
    for (index, target) in quote_targets.iter().enumerate() {
        if let Some(group) = groups
            .iter_mut()
            .find(|(raindex, _)| *raindex == target.raindex)
        {
            group.1.push((index, target.clone()));
        } else {
            groups.push((target.raindex, vec![(index, target.clone())]));
        }
    }

    let group_futures = groups.into_iter().map(|(_, indexed_targets)| {
        let providers = providers.to_vec();
        let provider_setup_failures = provider_setup_failures.clone();
        async move {
            let mut out = Vec::with_capacity(indexed_targets.len());
            let (indexes, targets): (Vec<usize>, Vec<QuoteTarget>) =
                indexed_targets.into_iter().unzip();
            for chunk_start in (0..targets.len()).step_by(chunk_size) {
                let chunk_end = (chunk_start + chunk_size).min(targets.len());
                let chunk = &targets[chunk_start..chunk_end];
                let mut failures = provider_setup_failures.clone();
                let mut chunk_results = None;
                for (rpc, provider) in &providers {
                    match quote_chunk_with_probe_and_split(
                        chunk,
                        provider,
                        block_number,
                        counterparty,
                        registry,
                    )
                    .await
                    {
                        Ok(results) => {
                            chunk_results = Some(results);
                            break;
                        }
                        Err(error) => failures.push(format!("{rpc}: {error}")),
                    }
                }
                let chunk_results = chunk_results.unwrap_or_else(|| {
                    let message = format!("all quote RPCs failed: {}", failures.join("; "));
                    (0..chunk.len())
                        .map(|_| Err(FailedQuote::CorruptReturnData(message.clone())))
                        .collect()
                });
                for (offset, result) in chunk_results.into_iter().enumerate() {
                    out.push((indexes[chunk_start + offset], result));
                }
            }
            Ok::<Vec<(usize, QuoteResult)>, Error>(out)
        }
    });

    let mut results: Vec<Option<QuoteResult>> = Vec::with_capacity(quote_targets.len());
    results.resize_with(quote_targets.len(), || None);
    for group_result in join_all(group_futures).await {
        for (index, result) in group_result? {
            results[index] = Some(result);
        }
    }
    Ok(results.into_iter().map(Option::unwrap).collect())
}

#[cfg(target_family = "wasm")]
async fn batch_quote_with_provider(
    quote_targets: &[QuoteTarget],
    provider: &ReadProvider,
    block_number: Option<u64>,
    counterparty: Address,
    registry: Option<&dyn ErrorRegistry>,
    chunk_size: Option<usize>,
) -> Result<Vec<QuoteResult>, Error> {
    let chunk_size = normalize_chunk_size(chunk_size);

    // Group by raindex, preserving the original index of each target so we
    // can scatter results back into the final vector in input order.
    let mut groups: Vec<(Address, Vec<(usize, QuoteTarget)>)> = Vec::new();
    for (i, target) in quote_targets.iter().enumerate() {
        if let Some(group) = groups.iter_mut().find(|(ob, _)| *ob == target.raindex) {
            group.1.push((i, target.clone()));
        } else {
            groups.push((target.raindex, vec![(i, target.clone())]));
        }
    }

    // For each raindex group, run chunked bisected multicalls concurrently.
    let group_futures = groups.into_iter().map(|(_ob, indexed_targets)| {
        let provider = provider.clone();
        async move {
            let mut out: Vec<(usize, QuoteResult)> = Vec::with_capacity(indexed_targets.len());
            let (indexes, targets): (Vec<usize>, Vec<QuoteTarget>) =
                indexed_targets.into_iter().unzip();

            for chunk_start in (0..targets.len()).step_by(chunk_size) {
                let chunk_end = (chunk_start + chunk_size).min(targets.len());
                let chunk = &targets[chunk_start..chunk_end];
                let chunk_results = quote_chunk_with_probe_and_split(
                    chunk,
                    &provider,
                    block_number,
                    counterparty,
                    registry,
                )
                .await?;
                for (offset, result) in chunk_results.into_iter().enumerate() {
                    out.push((indexes[chunk_start + offset], result));
                }
            }
            Ok::<Vec<(usize, QuoteResult)>, Error>(out)
        }
    });

    let group_outputs = join_all(group_futures).await;

    let mut results: Vec<Option<QuoteResult>> = Vec::with_capacity(quote_targets.len());
    results.resize_with(quote_targets.len(), || None);
    for group_result in group_outputs {
        for (idx, r) in group_result? {
            results[idx] = Some(r);
        }
    }
    Ok(results.into_iter().map(|r| r.unwrap()).collect())
}

#[cfg(not(target_family = "wasm"))]
#[cfg(test)]
mod tests {
    use super::*;
    use alloy::json_abi::Error as AlloyError;
    use alloy::primitives::Bytes;
    use alloy::sol_types::{SolCall, SolValue};
    use httpmock::{Method::POST, MockServer};
    use rain_error_decoding::ErrorRegistry;
    use rain_math_float::Float;
    use raindex_bindings::IRaindexV7::{quote3Call, quote3Return};
    use serde_json::json;

    #[test]
    fn test_normalize_chunk_size_defaults_to_16() {
        assert_eq!(normalize_chunk_size(None), 16);
    }

    #[test]
    fn test_normalize_chunk_size_coerces_zero_to_one() {
        assert_eq!(normalize_chunk_size(Some(0)), 1);
    }

    // Encode a single `quote3Return` into its ABI-encoded form (this is what
    // ends up as one element of the outer `bytes[]` returned by OZ Multicall).
    fn encode_quote3_return_bytes(exists: bool, output_max: Float, io_ratio: Float) -> Bytes {
        Bytes::from(quote3Call::abi_encode_returns(&quote3Return {
            exists,
            outputMax: output_max.get_inner(),
            ioRatio: io_ratio.get_inner(),
        }))
    }

    // Encode a `multicall(bytes[]) -> bytes[]` return value as the outer
    // eth_call return bytes, given per-inner encoded returns.
    fn encode_multicall_return(inner: Vec<Bytes>) -> String {
        let encoded = <Vec<Bytes> as SolValue>::abi_encode(&inner);
        alloy::hex::encode_prefixed(encoded)
    }

    fn mock_eth_call_static(server: &MockServer, path: &str, result_hex: &str) {
        let result = result_hex.to_string();
        server.mock(|when, then| {
            when.method(POST).path(path);
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": result,
            }));
        });
    }

    fn mock_eth_call_revert(server: &MockServer, path: &str, revert_hex: &str) {
        let data = revert_hex.to_string();
        server.mock(|when, then| {
            when.method(POST).path(path);
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": {
                    "code": 3,
                    "message": "execution reverted",
                    "data": data,
                }
            }));
        });
    }

    #[tokio::test]
    async fn test_batch_quote_empty_returns_empty() {
        let result = batch_quote(
            &[],
            vec!["http://localhost:1".to_string()],
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_batch_quote_without_rpcs_returns_explicit_aligned_failures() {
        let result = batch_quote(
            &[QuoteTarget::default()],
            Vec::new(),
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert!(matches!(
            &result[0],
            Err(FailedQuote::CorruptReturnData(message))
                if message.contains("no RPC URLs configured")
        ));
    }

    #[tokio::test]
    async fn test_batch_quote_transport_failure_returns_aligned_failures_without_bisection() {
        let rpc_server = MockServer::start_async().await;
        let failure = rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.status(429).body("rate limited");
        });

        let result = batch_quote(
            &[QuoteTarget::default(), QuoteTarget::default()],
            vec![rpc_server.url("/rpc").to_string()],
            Some(1),
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|result| matches!(
            result,
            Err(FailedQuote::CorruptReturnData(message))
                if message.contains("all quote RPCs failed")
        )));
        failure.assert_hits(1);
    }

    #[tokio::test]
    async fn test_batch_quote_applies_timeout_per_chunk_request() {
        let rpc_server = MockServer::start_async().await;
        let one = Float::parse("1".to_string()).unwrap();
        let response = encode_multicall_return(vec![encode_quote3_return_bytes(true, one, one)]);
        let quote = rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.delay(std::time::Duration::from_millis(1_600))
                .json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": response,
                }));
        });

        let results = batch_quote(
            &[QuoteTarget::default(), QuoteTarget::default()],
            vec![rpc_server.url("/rpc")],
            Some(1),
            Address::ZERO,
            None,
            Some(1),
        )
        .await
        .unwrap();

        assert!(results.iter().all(Result::is_ok));
        quote.assert_hits(2);
    }

    #[tokio::test]
    async fn test_batch_quote_cycles_to_next_rpc_after_transport_failure() {
        let failed_server = MockServer::start_async().await;
        let failure = failed_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.status(429).body("rate limited");
        });
        let healthy_server = MockServer::start_async().await;
        let healthy = healthy_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_multicall_return(vec![encode_quote3_return_bytes(
                    true,
                    Float::parse("2".into()).unwrap(),
                    Float::parse("3".into()).unwrap(),
                )]),
            }));
        });

        let result = batch_quote(
            &[QuoteTarget::default()],
            vec![failed_server.url("/rpc"), healthy_server.url("/rpc")],
            Some(1),
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!(
            result[0].is_ok(),
            "unexpected quote result: {:?}",
            result[0]
        );
        failure.assert_hits(1);
        healthy.assert_hits(1);
    }

    #[tokio::test]
    async fn test_batch_quote_cycles_to_next_rpc_after_timeout() {
        let stalled_server = MockServer::start_async().await;
        let stalled = stalled_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.delay(std::time::Duration::from_millis(
                RPC_ATTEMPT_TIMEOUT_MS + 200,
            ))
            .json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x",
            }));
        });
        let healthy_server = MockServer::start_async().await;
        let healthy = healthy_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_multicall_return(vec![encode_quote3_return_bytes(
                    true,
                    Float::parse("2".into()).unwrap(),
                    Float::parse("3".into()).unwrap(),
                )]),
            }));
        });

        let result = batch_quote(
            &[QuoteTarget::default()],
            vec![stalled_server.url("/rpc"), healthy_server.url("/rpc")],
            Some(1),
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert!(result[0].is_ok());
        stalled.assert_hits(1);
        healthy.assert_hits(1);
    }

    #[tokio::test]
    async fn test_batch_quote_cycles_to_next_rpc_after_malformed_response() {
        let malformed_server = MockServer::start_async().await;
        let malformed = malformed_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x",
            }));
        });
        let healthy_server = MockServer::start_async().await;
        let healthy = healthy_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_multicall_return(vec![encode_quote3_return_bytes(
                    true,
                    Float::parse("2".into()).unwrap(),
                    Float::parse("3".into()).unwrap(),
                )]),
            }));
        });

        let result = batch_quote(
            &[QuoteTarget::default()],
            vec![malformed_server.url("/rpc"), healthy_server.url("/rpc")],
            Some(1),
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert!(result[0].is_ok());
        malformed.assert_hits(1);
        healthy.assert_hits(1);
    }

    #[tokio::test]
    async fn test_batch_quote_cycles_to_next_rpc_after_malformed_url() {
        let healthy_server = MockServer::start_async().await;
        let healthy = healthy_server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": encode_multicall_return(vec![encode_quote3_return_bytes(
                    true,
                    Float::parse("2".into()).unwrap(),
                    Float::parse("3".into()).unwrap(),
                )]),
            }));
        });

        let result = batch_quote(
            &[QuoteTarget::default()],
            vec!["not a URL".into(), healthy_server.url("/rpc")],
            Some(1),
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert!(result[0].is_ok());
        healthy.assert_hits(1);
    }

    #[tokio::test]
    async fn test_batch_quote_success() {
        let rpc_server = MockServer::start_async().await;
        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        // One target → one multicall call with a single inner return.
        let inner = vec![encode_quote3_return_bytes(true, one, two)];
        mock_eth_call_static(&rpc_server, "/rpc", &encode_multicall_return(inner));

        let quote_targets = vec![QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        let q = result[0].as_ref().unwrap();
        assert!(q.max_output.eq(one).unwrap());
        assert!(q.ratio.eq(two).unwrap());
    }

    #[tokio::test]
    async fn test_batch_quote_multi_target_success() {
        let rpc_server = MockServer::start_async().await;
        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();
        let three = Float::parse("3".to_string()).unwrap();
        let four = Float::parse("4".to_string()).unwrap();

        let inner = vec![
            encode_quote3_return_bytes(true, one, two),
            encode_quote3_return_bytes(true, three, four),
        ];
        mock_eth_call_static(&rpc_server, "/rpc", &encode_multicall_return(inner));

        let quote_targets = vec![QuoteTarget::default(), QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 2);
        let q0 = result[0].as_ref().unwrap();
        let q1 = result[1].as_ref().unwrap();
        assert!(q0.max_output.eq(one).unwrap());
        assert!(q0.ratio.eq(two).unwrap());
        assert!(q1.max_output.eq(three).unwrap());
        assert!(q1.ratio.eq(four).unwrap());
    }

    #[tokio::test]
    async fn test_batch_quote_non_existent_order() {
        let rpc_server = MockServer::start_async().await;
        let zero = Float::parse("0".to_string()).unwrap();

        let inner = vec![encode_quote3_return_bytes(false, zero, zero)];
        mock_eth_call_static(&rpc_server, "/rpc", &encode_multicall_return(inner));

        let quote_targets = vec![QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Err(FailedQuote::NonExistent)));
    }

    #[tokio::test]
    async fn test_batch_quote_revert_decoded_via_registry() {
        let rpc_server = MockServer::start_async().await;

        // 0x734bc71c -> TokenSelfTrade(). OZ Multicall bubbles this up as the
        // outer revert data.
        mock_eth_call_revert(&rpc_server, "/rpc", "0x734bc71c");

        struct FakeRegistry;
        #[async_trait::async_trait]
        impl ErrorRegistry for FakeRegistry {
            async fn lookup(
                &self,
                selector: [u8; 4],
            ) -> Result<Vec<AlloyError>, rain_error_decoding::AbiDecodeFailedErrors> {
                if selector == [0x73, 0x4b, 0xc7, 0x1c] {
                    Ok(vec!["TokenSelfTrade()".parse().unwrap()])
                } else {
                    Ok(vec![])
                }
            }
        }

        let quote_targets = vec![QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            Some(&FakeRegistry),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!(matches!(
            &result[0],
            Err(FailedQuote::RevertError(
                rain_error_decoding::AbiDecodedErrorType::Known { name, .. }
            )) if name == "TokenSelfTrade"
        ));
    }

    #[tokio::test]
    async fn test_batch_quote_revert_unknown_selector() {
        let rpc_server = MockServer::start_async().await;
        mock_eth_call_revert(&rpc_server, "/rpc", "0xff00ff00");

        struct EmptyRegistry;
        #[async_trait::async_trait]
        impl ErrorRegistry for EmptyRegistry {
            async fn lookup(
                &self,
                _selector: [u8; 4],
            ) -> Result<Vec<AlloyError>, rain_error_decoding::AbiDecodeFailedErrors> {
                Ok(vec![])
            }
        }

        let quote_targets = vec![QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            Some(&EmptyRegistry),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!(matches!(
            &result[0],
            Err(FailedQuote::RevertError(
                rain_error_decoding::AbiDecodedErrorType::Unknown(_)
            ))
        ));
    }

    #[tokio::test]
    async fn test_batch_quote_invalid_rpc_url_returns_aligned_failure() {
        let quote_targets = vec![QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec!["this should break".to_string()],
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!(matches!(
            &result[0],
            Err(FailedQuote::CorruptReturnData(message))
                if message.contains("all quote RPCs failed")
        ));
    }

    #[tokio::test]
    async fn test_batch_quote_counterparty_is_sent_as_from() {
        let rpc_server = MockServer::start_async().await;
        let cp = "0xaaaaaaaaaabbbbbbbbbbccccccccccdddddddddd"
            .parse::<Address>()
            .unwrap();

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();
        let inner = vec![encode_quote3_return_bytes(true, one, two)];
        let ret_hex = encode_multicall_return(inner);

        // alloy serialises `from` lowercased in JSON-RPC params; asserting the
        // body contains the hex-encoded counterparty verifies threading.
        let expected_from = format!("{cp:#x}");
        let mock = rpc_server.mock(|when, then| {
            when.method(POST).path("/rpc").body_contains(&expected_from);
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": ret_hex,
            }));
        });

        let quote_targets = vec![QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            cp,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!(result[0].is_ok());
        mock.assert();
    }

    // When every target in a chunk reverts with the same error, the
    // bisection isolates each singleton and attributes the revert per target.
    #[tokio::test]
    async fn test_batch_quote_chunk_uniform_revert_bisects_to_singletons() {
        let rpc_server = MockServer::start_async().await;

        // Every eth_call to this raindex reverts with TokenSelfTrade.
        mock_eth_call_revert(&rpc_server, "/rpc", "0x734bc71c");

        struct FakeRegistry;
        #[async_trait::async_trait]
        impl ErrorRegistry for FakeRegistry {
            async fn lookup(
                &self,
                selector: [u8; 4],
            ) -> Result<Vec<AlloyError>, rain_error_decoding::AbiDecodeFailedErrors> {
                if selector == [0x73, 0x4b, 0xc7, 0x1c] {
                    Ok(vec!["TokenSelfTrade()".parse().unwrap()])
                } else {
                    Ok(vec![])
                }
            }
        }

        let quote_targets = vec![QuoteTarget::default(), QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            Some(&FakeRegistry),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 2);
        for r in &result {
            assert!(matches!(
                r,
                Err(FailedQuote::RevertError(
                    rain_error_decoding::AbiDecodedErrorType::Known { name, .. }
                )) if name == "TokenSelfTrade"
            ));
        }
    }

    // ---------------------------------------------------------------------
    // Batch-assembly coverage: raindex grouping, input-order scatter, chunk
    // boundary stepping, mixed-revert bisection offsets, length-mismatch guard.
    // ---------------------------------------------------------------------

    use alloy::primitives::U256;
    use raindex_bindings::IRaindexV7::QuoteV3;

    // Build a `QuoteTarget` with a specific raindex address and a distinctive
    // `inputIOIndex`. The index is ABI-encoded into the inner `quote3` calldata,
    // so a large/unusual value (e.g. 0xAAAA) gives a hex substring that uniquely
    // identifies this target inside the outer multicall request body. This lets
    // a mock respond differently per-target / per-chunk without per-target paths.
    fn target_with(raindex: Address, input_index: u64) -> QuoteTarget {
        QuoteTarget {
            raindex,
            quote_config: QuoteV3 {
                inputIOIndex: U256::from(input_index),
                ..Default::default()
            },
        }
    }

    // Mock an eth_call that only matches when the request body contains
    // `needle` (a hex substring identifying a specific inner quote3 call or the
    // raindex `to` address), returning the given multicall hex.
    fn mock_eth_call_when_body_contains(
        server: &MockServer,
        path: &str,
        needle: &str,
        result_hex: &str,
    ) {
        let result = result_hex.to_string();
        let needle = needle.to_string();
        server.mock(|when, then| {
            when.method(POST).path(path).body_contains(&needle);
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": result,
            }));
        });
    }

    // Two targets pointing at two DISTINCT raindex addresses fall into separate
    // groups (each gets its own multicall). The per-group results must scatter
    // back into the original input positions. We key each group's mock on the
    // raindex `to` address in the request body so they cannot be confused.
    #[tokio::test]
    async fn test_batch_quote_groups_by_raindex_and_scatters_in_input_order() {
        let rpc_server = MockServer::start_async().await;

        let raindex_a = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse::<Address>()
            .unwrap();
        let raindex_b = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .parse::<Address>()
            .unwrap();

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();
        let three = Float::parse("3".to_string()).unwrap();
        let four = Float::parse("4".to_string()).unwrap();

        // Group A (raindex_a) -> quote (1, 2); Group B (raindex_b) -> quote (3, 4).
        let ret_a = encode_multicall_return(vec![encode_quote3_return_bytes(true, one, two)]);
        let ret_b = encode_multicall_return(vec![encode_quote3_return_bytes(true, three, four)]);
        mock_eth_call_when_body_contains(&rpc_server, "/rpc", "aaaaaaaaaaaaaaaaaa", &ret_a);
        mock_eth_call_when_body_contains(&rpc_server, "/rpc", "bbbbbbbbbbbbbbbbbb", &ret_b);

        // Input order: A first, then B. Results must come back A, then B.
        let quote_targets = vec![target_with(raindex_a, 0), target_with(raindex_b, 0)];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 2);
        let q0 = result[0].as_ref().unwrap();
        let q1 = result[1].as_ref().unwrap();
        // Position 0 carries group A's quote (1, 2); position 1 carries group B's (3, 4).
        assert!(q0.max_output.eq(one).unwrap());
        assert!(q0.ratio.eq(two).unwrap());
        assert!(q1.max_output.eq(three).unwrap());
        assert!(q1.ratio.eq(four).unwrap());
    }

    // With chunk_size smaller than the group size, the group is split into
    // consecutive chunks. Three same-raindex targets and chunk_size = 2 ->
    // chunk one holds targets [0, 1], chunk two holds target [2]. Each chunk is
    // a separate multicall whose returned `bytes[]` length must equal the chunk
    // size, and results must reassemble in input order.
    #[tokio::test]
    async fn test_batch_quote_chunks_group_and_preserves_order() {
        let rpc_server = MockServer::start_async().await;
        let raindex = "0xcccccccccccccccccccccccccccccccccccccccc"
            .parse::<Address>()
            .unwrap();

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();
        let three = Float::parse("3".to_string()).unwrap();
        let four = Float::parse("4".to_string()).unwrap();
        let five = Float::parse("5".to_string()).unwrap();
        let six = Float::parse("6".to_string()).unwrap();

        // Distinctive indices so each chunk's multicall body is identifiable.
        let idx0 = 0xAAAAu64;
        let idx1 = 0xBBBBu64;
        let idx2 = 0xCCCCu64;
        let needle0 = format!("{idx0:064x}");
        let needle1 = format!("{idx1:064x}");
        let needle2 = format!("{idx2:064x}");

        // Chunk one (targets 0 and 1) returns TWO inner results; matched by the
        // presence of target 1's index (only present in the first chunk).
        let ret_chunk1 = encode_multicall_return(vec![
            encode_quote3_return_bytes(true, one, two),
            encode_quote3_return_bytes(true, three, four),
        ]);
        mock_eth_call_when_body_contains(&rpc_server, "/rpc", &needle1, &ret_chunk1);
        // Chunk two (target 2 only) returns ONE inner result; matched by target
        // 2's index, which never appears in the first chunk.
        let ret_chunk2 = encode_multicall_return(vec![encode_quote3_return_bytes(true, five, six)]);
        mock_eth_call_when_body_contains(&rpc_server, "/rpc", &needle2, &ret_chunk2);
        // Target 0's index is bound for parity with the other chunk needles;
        // chunk one is matched by target 1's index, so it is unused here.
        let _ = &needle0;

        let quote_targets = vec![
            target_with(raindex, idx0),
            target_with(raindex, idx1),
            target_with(raindex, idx2),
        ];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            None,
            Some(2),
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 3);
        let q0 = result[0].as_ref().unwrap();
        let q1 = result[1].as_ref().unwrap();
        let q2 = result[2].as_ref().unwrap();
        assert!(q0.max_output.eq(one).unwrap());
        assert!(q0.ratio.eq(two).unwrap());
        assert!(q1.max_output.eq(three).unwrap());
        assert!(q1.ratio.eq(four).unwrap());
        assert!(q2.max_output.eq(five).unwrap());
        assert!(q2.ratio.eq(six).unwrap());
    }

    #[tokio::test]
    async fn test_batch_quote_retries_only_the_failed_chunk_on_the_next_rpc() {
        let first_rpc = MockServer::start_async().await;
        let second_rpc = MockServer::start_async().await;
        let raindex = "0xcccccccccccccccccccccccccccccccccccccccc"
            .parse::<Address>()
            .unwrap();
        let first_index = 0xAAAAu64;
        let second_index = 0xBBBBu64;
        let first_needle = format!("{first_index:064x}");
        let second_needle = format!("{second_index:064x}");
        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();
        let first_return =
            encode_multicall_return(vec![encode_quote3_return_bytes(true, one, one)]);
        let second_return =
            encode_multicall_return(vec![encode_quote3_return_bytes(true, two, two)]);

        let first_success = first_rpc.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(first_needle.clone());
            then.json_body(json!({"jsonrpc": "2.0", "id": 1, "result": first_return}));
        });
        let first_failure = first_rpc.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(second_needle.clone());
            then.status(429).body("rate limited");
        });
        let second_success = second_rpc.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(second_needle.clone());
            then.json_body(json!({"jsonrpc": "2.0", "id": 1, "result": second_return}));
        });

        let results = batch_quote(
            &[
                target_with(raindex, first_index),
                target_with(raindex, second_index),
            ],
            vec![
                first_rpc.url("/rpc").to_string(),
                second_rpc.url("/rpc").to_string(),
            ],
            None,
            Address::ZERO,
            None,
            Some(1),
        )
        .await
        .unwrap();

        assert!(results[0].as_ref().unwrap().ratio.eq(one).unwrap());
        assert!(results[1].as_ref().unwrap().ratio.eq(two).unwrap());
        first_success.assert_hits(1);
        first_failure.assert_hits(1);
        second_success.assert_hits(1);
    }

    // A multicall whose returned `bytes[]` length does not match the number of
    // targets in the chunk is rejected by the length-mismatch guard. With a
    // single target, the guard fires inside `quote_chunk_once`; the singleton
    // path surfaces it as a per-target `CorruptReturnData` carrying the mismatch
    // detail.
    #[tokio::test]
    async fn test_batch_quote_multicall_length_mismatch_is_rejected() {
        let rpc_server = MockServer::start_async().await;

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();
        // ONE target but TWO returned inner elements.
        let ret = encode_multicall_return(vec![
            encode_quote3_return_bytes(true, one, two),
            encode_quote3_return_bytes(true, one, two),
        ]);
        mock_eth_call_static(&rpc_server, "/rpc", &ret);

        let quote_targets = vec![QuoteTarget::default()];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        match &result[0] {
            Err(FailedQuote::CorruptReturnData(msg)) => {
                assert!(
                    msg.contains("length mismatch"),
                    "expected length-mismatch detail, got: {msg}"
                );
            }
            other => panic!("expected CorruptReturnData length mismatch, got: {other:?}"),
        }
    }

    // Mixed chunk: one target quotes successfully, the other reverts. The full
    // chunk multicall (both inner calls) reverts because OZ Multicall bubbles
    // the first failing inner call. Bisection splits to singletons and places
    // the successful quote at its input offset and the revert at the other.
    #[tokio::test]
    async fn test_batch_quote_mixed_chunk_bisects_success_and_revert_by_offset() {
        let rpc_server = MockServer::start_async().await;
        let raindex = "0xdddddddddddddddddddddddddddddddddddddddd"
            .parse::<Address>()
            .unwrap();

        let one = Float::parse("1".to_string()).unwrap();
        let two = Float::parse("2".to_string()).unwrap();

        let ok_idx = 0xAAAAu64;
        let revert_idx = 0xBBBBu64;
        let ok_needle = format!("{ok_idx:064x}");
        let revert_needle = format!("{revert_idx:064x}");

        // Singleton for the OK target: matches a body containing the OK index
        // but NOT the revert index.
        struct FakeRegistry;
        #[async_trait::async_trait]
        impl ErrorRegistry for FakeRegistry {
            async fn lookup(
                &self,
                selector: [u8; 4],
            ) -> Result<Vec<AlloyError>, rain_error_decoding::AbiDecodeFailedErrors> {
                if selector == [0x73, 0x4b, 0xc7, 0x1c] {
                    Ok(vec!["TokenSelfTrade()".parse().unwrap()])
                } else {
                    Ok(vec![])
                }
            }
        }

        // Resolution is by definition order (httpmock scans mocks ascending by
        // id and takes the first match). Register the REVERT mock first, keyed
        // on the revert index: it matches the full 2-call chunk (which contains
        // both indices) AND the revert singleton, and wins for both. The OK
        // singleton body contains only the OK index, so this revert mock does
        // NOT match it.
        rpc_server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(revert_needle.clone());
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": { "code": 3, "message": "execution reverted", "data": "0x734bc71c" }
            }));
        });
        // OK singleton: body contains the OK index but not the revert index, so
        // only this mock matches it.
        let ok_ret = encode_multicall_return(vec![encode_quote3_return_bytes(true, one, two)]);
        rpc_server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(ok_needle.clone());
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": ok_ret,
            }));
        });

        // Input order: OK target at index 0, revert target at index 1.
        let quote_targets = vec![
            target_with(raindex, ok_idx),
            target_with(raindex, revert_idx),
        ];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            Some(&FakeRegistry),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 2);
        // Offset 0 -> the successful quote.
        let q0 = result[0].as_ref().unwrap();
        assert!(q0.max_output.eq(one).unwrap());
        assert!(q0.ratio.eq(two).unwrap());
        // Offset 1 -> the revert, decoded as TokenSelfTrade.
        assert!(matches!(
            &result[1],
            Err(FailedQuote::RevertError(
                rain_error_decoding::AbiDecodedErrorType::Known { name, .. }
            )) if name == "TokenSelfTrade"
        ));
    }

    // Bisection where a multi-element SUB-chunk succeeds wholesale. Four
    // same-raindex targets: the first pair reverts, the second pair quotes.
    // The full 4-call chunk reverts (OZ bubbles the first revert), so bisection
    // splits into [0,2) and [2,4). The [2,4) half succeeds as a single
    // 2-element multicall, exercising the Ok branch of the bisection loop with a
    // chunk whose `start = 2` and offsets 0 and 1. Those successes land at
    // resolved[2] and resolved[3] (i.e. `resolved[start + offset]`). The [0,2)
    // half reverts and bisects to singletons.
    #[tokio::test]
    async fn test_batch_quote_bisection_multi_element_ok_subchunk_lands_at_offset() {
        let rpc_server = MockServer::start_async().await;
        let raindex = "0xee0000000000000000000000000000000000ee00"
            .parse::<Address>()
            .unwrap();

        let three = Float::parse("3".to_string()).unwrap();
        let four = Float::parse("4".to_string()).unwrap();
        let five = Float::parse("5".to_string()).unwrap();
        let six = Float::parse("6".to_string()).unwrap();

        // Revert pair indices and success pair indices, each distinctive.
        let r0 = 0xA0A0u64;
        let r1 = 0xA1A1u64;
        let s2 = 0xB2B2u64;
        let s3 = 0xB3B3u64;
        let r0_needle = format!("{r0:064x}");
        let r1_needle = format!("{r1:064x}");
        let s2_needle = format!("{s2:064x}");
        let s3_needle = format!("{s3:064x}");

        struct FakeRegistry;
        #[async_trait::async_trait]
        impl ErrorRegistry for FakeRegistry {
            async fn lookup(
                &self,
                selector: [u8; 4],
            ) -> Result<Vec<AlloyError>, rain_error_decoding::AbiDecodeFailedErrors> {
                if selector == [0x73, 0x4b, 0xc7, 0x1c] {
                    Ok(vec!["TokenSelfTrade()".parse().unwrap()])
                } else {
                    Ok(vec![])
                }
            }
        }

        // Resolution is first-match by definition order. Register revert mocks
        // FIRST keyed on each revert index: a body containing r1 (full chunk,
        // [0,2) half, r1 singleton) reverts; a body containing r0 (r0 singleton)
        // reverts. The full chunk also contains s2/s3, but the r1 revert mock is
        // earlier so it wins. The [2,4) half contains only s2/s3 (no r0/r1), so
        // no revert mock matches it.
        rpc_server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(r1_needle.clone());
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": { "code": 3, "message": "execution reverted", "data": "0x734bc71c" }
            }));
        });
        rpc_server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(r0_needle.clone());
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": { "code": 3, "message": "execution reverted", "data": "0x734bc71c" }
            }));
        });
        // Success mock for the [2,4) half: matches only when BOTH s2 and s3 are
        // present (the 2-element sub-chunk). Returns a 2-element multicall.
        let ok_pair = encode_multicall_return(vec![
            encode_quote3_return_bytes(true, three, four),
            encode_quote3_return_bytes(true, five, six),
        ]);
        rpc_server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_contains(s2_needle.clone())
                .body_contains(s3_needle.clone());
            then.json_body(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": ok_pair,
            }));
        });

        // Input order: revert pair first (slots 0,1), success pair second (2,3).
        let quote_targets = vec![
            target_with(raindex, r0),
            target_with(raindex, r1),
            target_with(raindex, s2),
            target_with(raindex, s3),
        ];
        let result = batch_quote(
            &quote_targets,
            vec![rpc_server.url("/rpc").to_string()],
            None,
            Address::ZERO,
            Some(&FakeRegistry),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.len(), 4);
        // Slots 0 and 1: the reverting pair.
        for r in &result[0..2] {
            assert!(
                matches!(
                    r,
                    Err(FailedQuote::RevertError(
                        rain_error_decoding::AbiDecodedErrorType::Known { name, .. }
                    )) if name == "TokenSelfTrade"
                ),
                "expected revert in slots 0/1, got: {r:?}"
            );
        }
        // Slots 2 and 3: the successful pair, in order (3,4) then (5,6).
        let q2 = result[2].as_ref().unwrap();
        let q3 = result[3].as_ref().unwrap();
        assert!(q2.max_output.eq(three).unwrap());
        assert!(q2.ratio.eq(four).unwrap());
        assert!(q3.max_output.eq(five).unwrap());
        assert!(q3.ratio.eq(six).unwrap());
    }
}
