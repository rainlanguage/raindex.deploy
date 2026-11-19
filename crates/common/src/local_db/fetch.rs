use super::LocalDbError;
use crate::{
    retry::retry_with_backoff,
    rpc_client::{LogEntryResponse, RpcClient, RpcClientError},
};
use alloy::primitives::{Address, U256};
use alloy::rpc::types::Filter;
use futures::{StreamExt, TryStreamExt};
use raindex_bindings::topics::{RAINDEX_EVENT_TOPICS, STORE_SET_TOPICS};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FetchConfigError {
    #[error("chunk_size must be greater than zero (got {0})")]
    ChunkSizeZero(u64),

    #[error("max_concurrent_requests must be greater than zero (got {0})")]
    MaxConcurrentRequestsZero(usize),

    #[error("max_concurrent_blocks must be greater than zero (got {0})")]
    MaxConcurrentBlocksZero(usize),

    #[error("max_retry_attempts must be greater than zero (got {0})")]
    MaxRetryAttemptsZero(usize),
}

#[derive(Debug, Clone)]
pub struct FetchConfig {
    chunk_size: u64,
    max_concurrent_requests: usize,
    max_concurrent_blocks: usize,
    max_retry_attempts: usize,
    retry_delay_ms: u64,
    rate_limit_delay_ms: u64,
}

impl FetchConfig {
    pub const DEFAULT_CHUNK_SIZE: u64 = 5000;
    pub const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 10;
    pub const DEFAULT_MAX_CONCURRENT_BLOCKS: usize = 14;
    pub const DEFAULT_MAX_RETRY_ATTEMPTS: usize = 3;
    pub const DEFAULT_RETRY_DELAY_MS: u64 = 500;
    pub const DEFAULT_RATE_LIMIT_DELAY_MS: u64 = 0;

    pub fn new(
        chunk_size: u64,
        max_concurrent_requests: usize,
        max_concurrent_blocks: usize,
        max_retry_attempts: usize,
        retry_delay_ms: u64,
        rate_limit_delay_ms: u64,
    ) -> Result<Self, FetchConfigError> {
        if chunk_size == 0 {
            return Err(FetchConfigError::ChunkSizeZero(chunk_size));
        }
        if max_concurrent_requests == 0 {
            return Err(FetchConfigError::MaxConcurrentRequestsZero(
                max_concurrent_requests,
            ));
        }
        if max_concurrent_blocks == 0 {
            return Err(FetchConfigError::MaxConcurrentBlocksZero(
                max_concurrent_blocks,
            ));
        }
        if max_retry_attempts == 0 {
            return Err(FetchConfigError::MaxRetryAttemptsZero(max_retry_attempts));
        }

        Ok(Self {
            chunk_size,
            max_concurrent_requests,
            max_concurrent_blocks,
            max_retry_attempts,
            retry_delay_ms,
            rate_limit_delay_ms,
        })
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn max_concurrent_requests(&self) -> usize {
        self.max_concurrent_requests
    }

    pub fn max_concurrent_blocks(&self) -> usize {
        self.max_concurrent_blocks
    }

    pub fn max_retry_attempts(&self) -> usize {
        self.max_retry_attempts
    }

    pub fn retry_delay_ms(&self) -> u64 {
        self.retry_delay_ms
    }

    pub fn rate_limit_delay_ms(&self) -> u64 {
        self.rate_limit_delay_ms
    }
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            chunk_size: Self::DEFAULT_CHUNK_SIZE,
            max_concurrent_requests: Self::DEFAULT_MAX_CONCURRENT_REQUESTS,
            max_concurrent_blocks: Self::DEFAULT_MAX_CONCURRENT_BLOCKS,
            max_retry_attempts: Self::DEFAULT_MAX_RETRY_ATTEMPTS,
            retry_delay_ms: Self::DEFAULT_RETRY_DELAY_MS,
            rate_limit_delay_ms: Self::DEFAULT_RATE_LIMIT_DELAY_MS,
        }
    }
}

async fn collect_logs(
    rpc_client: &RpcClient,
    filter: &Filter,
    config: &FetchConfig,
) -> Result<Vec<LogEntryResponse>, LocalDbError> {
    filter.block_option.ensure_valid_block_range()?;

    if filter.address.is_empty() {
        return Ok(Vec::new());
    }

    let filters = build_log_filters(filter, config)?;
    if filters.is_empty() {
        return Ok(Vec::new());
    }

    let mut events = fetch_logs_for_filters(rpc_client, filters, config).await?;

    sort_events_by_block_and_log(&mut events);
    backfill_missing_timestamps(rpc_client, &mut events, config).await?;

    Ok(events)
}

pub async fn fetch_raindex_events(
    rpc_client: &RpcClient,
    address: Address,
    from_block: u64,
    to_block: u64,
    config: &FetchConfig,
) -> Result<Vec<LogEntryResponse>, LocalDbError> {
    let filter = Filter::new()
        .address(address)
        .from_block(from_block)
        .to_block(to_block)
        .event_signature(RAINDEX_EVENT_TOPICS.to_vec());
    collect_logs(rpc_client, &filter, config).await
}

pub async fn fetch_store_events(
    rpc_client: &RpcClient,
    addresses: &[Address],
    from_block: u64,
    to_block: u64,
    config: &FetchConfig,
) -> Result<Vec<LogEntryResponse>, LocalDbError> {
    let filter = Filter::new()
        .address(addresses.to_vec())
        .from_block(from_block)
        .to_block(to_block)
        .event_signature(STORE_SET_TOPICS.to_vec());
    collect_logs(rpc_client, &filter, config).await
}

async fn fetch_block_timestamps(
    rpc_client: &RpcClient,
    block_numbers: Vec<u64>,
    config: &FetchConfig,
) -> Result<HashMap<u64, U256>, LocalDbError> {
    if block_numbers.is_empty() {
        return Ok(HashMap::new());
    }

    let concurrency = config.max_concurrent_blocks();
    let retry_delay = config.retry_delay_ms();
    let rate_limit_delay = config.rate_limit_delay_ms();
    let results: Vec<Result<(u64, U256), LocalDbError>> = futures::stream::iter(block_numbers)
        .map(|block_number| {
            let client = rpc_client.clone();
            let max_attempts = config.max_retry_attempts();
            async move {
                let block_response = retry_with_backoff(
                    || {
                        let client = client.clone();
                        async move {
                            client
                                .get_block_by_number(block_number)
                                .await
                                .map_err(map_rpc_error)
                        }
                    },
                    max_attempts,
                    retry_delay,
                    rate_limit_delay,
                    should_retry_local_db_error,
                    is_rate_limited_local_db_error,
                )
                .await?;

                let block_data = block_response.ok_or_else(|| LocalDbError::MissingField {
                    field: "result".to_string(),
                })?;

                Ok((block_number, block_data.timestamp))
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    results.into_iter().collect()
}

async fn backfill_missing_timestamps(
    rpc_client: &RpcClient,
    events: &mut [LogEntryResponse],
    config: &FetchConfig,
) -> Result<(), LocalDbError> {
    let mut missing_blocks = HashSet::new();

    for event in events.iter() {
        let has_timestamp = event.block_timestamp.as_ref().is_some();

        if !has_timestamp {
            missing_blocks.insert(event.block_number.to::<u64>());
        }
    }

    if missing_blocks.is_empty() {
        return Ok(());
    }

    let block_numbers: Vec<u64> = missing_blocks.into_iter().collect();
    let timestamps = fetch_block_timestamps(rpc_client, block_numbers, config).await?;

    for event in events.iter_mut() {
        let has_timestamp = event.block_timestamp.as_ref().is_some();

        if has_timestamp {
            continue;
        }

        let block_number = event.block_number.to::<u64>();
        if let Some(timestamp) = timestamps.get(&block_number).cloned() {
            event.block_timestamp = Some(timestamp);
        }
    }

    Ok(())
}

fn build_log_filters(filter: &Filter, config: &FetchConfig) -> Result<Vec<Filter>, LocalDbError> {
    let chunk_size = config.chunk_size.max(1);
    let chunk_span = chunk_size.saturating_sub(1);
    let mut filters = Vec::new();

    let from_block = filter
        .block_option
        .get_from_block()
        .ok_or(LocalDbError::MissingBlockFilter("from".to_string()))?
        .as_number()
        .ok_or(LocalDbError::NonNumberBlockNumber("from".to_string()))?;
    let to_block = filter
        .block_option
        .get_to_block()
        .ok_or(LocalDbError::MissingBlockFilter("to".to_string()))?
        .as_number()
        .ok_or(LocalDbError::NonNumberBlockNumber("to".to_string()))?;

    for &address in filter.address.iter() {
        let mut current_block = from_block;
        while current_block <= to_block {
            let end = current_block.saturating_add(chunk_span).min(to_block);

            let all_topics = filter.topics.clone();
            let topics = all_topics
                .first()
                .ok_or(LocalDbError::MissingTopicsFilter)?;

            let mut filter = Filter::new()
                .address(address)
                .from_block(current_block)
                .to_block(end);
            filter = filter.event_signature(topics.clone());
            filters.push(filter);

            if end == to_block || end == u64::MAX {
                break;
            }

            current_block = end.saturating_add(1);
        }
    }

    Ok(filters)
}

async fn fetch_logs_for_filters(
    rpc_client: &RpcClient,
    filters: Vec<Filter>,
    config: &FetchConfig,
) -> Result<Vec<LogEntryResponse>, LocalDbError> {
    if filters.is_empty() {
        return Ok(Vec::new());
    }

    let concurrency = config.max_concurrent_requests();
    let retry_delay = config.retry_delay_ms();
    let rate_limit_delay = config.rate_limit_delay_ms();
    let results: Vec<Vec<LogEntryResponse>> = futures::stream::iter(filters)
        .map(|filter| {
            let client = rpc_client.clone();
            let max_attempts = config.max_retry_attempts();

            async move {
                let response = retry_with_backoff(
                    || {
                        let client = client.clone();
                        let filter = filter.clone();
                        async move { client.get_logs(&filter).await.map_err(map_rpc_error) }
                    },
                    max_attempts,
                    retry_delay,
                    rate_limit_delay,
                    should_retry_local_db_error,
                    is_rate_limited_local_db_error,
                )
                .await?;

                Ok::<_, LocalDbError>(response)
            }
        })
        .buffer_unordered(concurrency)
        .try_collect()
        .await?;

    Ok(results.into_iter().flatten().collect())
}

fn sort_events_by_block_and_log(events: &mut [LogEntryResponse]) {
    events.sort_by(|a, b| {
        let block_a = a.block_number.to::<u64>();
        let block_b = b.block_number.to::<u64>();
        let log_a = a.log_index.to::<u64>();
        let log_b = b.log_index.to::<u64>();
        block_a.cmp(&block_b).then_with(|| log_a.cmp(&log_b))
    });
}

fn map_rpc_error(error: RpcClientError) -> LocalDbError {
    match error {
        RpcClientError::JsonSerialization(err) => LocalDbError::JsonParse(err),
        other => LocalDbError::Rpc(other),
    }
}

fn should_retry_local_db_error(error: &LocalDbError) -> bool {
    matches!(error, LocalDbError::Rpc(_))
}

fn is_rate_limited_local_db_error(error: &LocalDbError) -> bool {
    matches!(error, LocalDbError::Rpc(RpcClientError::RateLimited { .. }))
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_family = "wasm"))]
    use super::*;

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn fetch_config_new_rejects_zero_values() {
        assert!(matches!(
            FetchConfig::new(0, 1, 1, 1, 0, 0),
            Err(FetchConfigError::ChunkSizeZero(0))
        ));
        assert!(matches!(
            FetchConfig::new(1, 0, 1, 1, 0, 0),
            Err(FetchConfigError::MaxConcurrentRequestsZero(0))
        ));
        assert!(matches!(
            FetchConfig::new(1, 1, 0, 1, 0, 0),
            Err(FetchConfigError::MaxConcurrentBlocksZero(0))
        ));
        assert!(matches!(
            FetchConfig::new(1, 1, 1, 0, 0, 0),
            Err(FetchConfigError::MaxRetryAttemptsZero(0))
        ));
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn is_rate_limited_detects_rate_limited_error() {
        assert!(is_rate_limited_local_db_error(&LocalDbError::Rpc(
            RpcClientError::RateLimited {
                message: "rate limited".to_string(),
            }
        )));
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn is_rate_limited_rejects_non_rate_limited_error() {
        assert!(!is_rate_limited_local_db_error(&LocalDbError::Rpc(
            RpcClientError::RpcError {
                message: "some error".to_string(),
            }
        )));
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn fetch_config_default_values_are_valid() {
        let default = FetchConfig::default();
        assert_eq!(default.chunk_size(), FetchConfig::DEFAULT_CHUNK_SIZE);
        assert_eq!(
            default.max_concurrent_requests(),
            FetchConfig::DEFAULT_MAX_CONCURRENT_REQUESTS
        );
        assert_eq!(
            default.max_concurrent_blocks(),
            FetchConfig::DEFAULT_MAX_CONCURRENT_BLOCKS
        );
        assert_eq!(
            default.max_retry_attempts(),
            FetchConfig::DEFAULT_MAX_RETRY_ATTEMPTS
        );
    }

    #[cfg(not(target_family = "wasm"))]
    mod tokio_tests {
        use super::*;
        use alloy::hex;
        use alloy::primitives::{Address, Bytes, B256, U256};
        use alloy::rpc::types::FilterBlockError;
        use alloy::sol_types::SolEvent;
        use httpmock::prelude::*;
        use raindex_bindings::{IInterpreterStoreV3::Set, IRaindexV7::AddOrderV3};
        use serde_json::json;
        use std::str::FromStr;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use url::Url;

        fn parse_u256(value: &str) -> U256 {
            let trimmed = value.trim();
            let digits = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
                .unwrap_or(trimmed);
            let (radix, literal) = if digits.len() != trimmed.len() {
                (16, digits)
            } else {
                (10, trimmed)
            };
            U256::from_str_radix(literal, radix).expect("valid u256 literal")
        }

        fn parse_b256(value: &str) -> B256 {
            let trimmed = value.trim_start_matches("0x").trim_start_matches("0X");
            if trimmed.is_empty() {
                return B256::ZERO;
            }
            let parsed = U256::from_str_radix(trimmed, 16).expect("valid b256 literal");
            B256::from(parsed)
        }

        fn make_log_entry_basic(block_number: &str, timestamp: Option<&str>) -> LogEntryResponse {
            LogEntryResponse {
                address: Address::from([0x12; 20]),
                topics: vec![Bytes::from(vec![0xab; 32])],
                data: Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]),
                block_number: parse_u256(block_number),
                block_timestamp: timestamp.map(parse_u256),
                transaction_hash: parse_b256("0x01"),
                transaction_index: "0x0".to_string(),
                block_hash: parse_b256("0x02"),
                log_index: U256::ZERO,
                removed: false,
            }
        }

        fn sample_block_response(number: &str, timestamp: Option<&str>) -> String {
            let hash = "0x0000000000000000000000000000000000000000000000000000000000010101";
            let mut block = json!({
                "mixHash": "0x01020304",
                "difficulty": "0x01",
                "extraData": "0x02",
                "gasLimit": "0xffff",
                "gasUsed": "0xff",
                "hash": hash,
                "logsBloom": "0x00",
                "miner": "0x0a",
                "nonce": "0x01",
                "number": number,
                "parentHash": "0x020202",
                "receiptsRoot": "0x030303",
                "sha3Uncles": "0x040404",
                "size": "0x1",
                "stateRoot": "0x050505",
                "timestamp": timestamp.unwrap_or("0x00"),
                "totalDifficulty": "0x02",
                "transactionsRoot": "0x060606",
                "uncles": [],
                "transactions": [],
            });
            if timestamp.is_none() {
                block.as_object_mut().unwrap().remove("timestamp");
            }
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": block
            })
            .to_string()
        }

        #[tokio::test]
        async fn fetch_raindex_events_fetches_and_sorts() {
            let server = MockServer::start();

            let response_body = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": [
                    {
                        "address": "0x0000000000000000000000000000000000000abc",
                        "topics": [
                            format!("0x{}", hex::encode(AddOrderV3::SIGNATURE_HASH))
                        ],
                        "data": "0xdeadbeef",
                        "blockNumber": "0x2",
                        "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000002",
                        "transactionIndex": "0x0",
                        "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000002",
                        "logIndex": "0x01",
                        "removed": false
                    },
                    {
                        "address": "0x0000000000000000000000000000000000000abc",
                        "topics": [
                            format!("0x{}", hex::encode(AddOrderV3::SIGNATURE_HASH))
                        ],
                        "data": "0xdeadbeef",
                        "blockNumber": "0x1",
                        "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
                        "transactionIndex": "0x0",
                        "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
                        "logIndex": "0x00",
                        "removed": false
                    }
                ]
            });

            let block_response = sample_block_response("0x1", Some("0x64"));
            let block_response_two = sample_block_response("0x2", Some("0x65"));

            server.mock(|when, then| {
                when.method(POST).path("/").body_contains("\"eth_getLogs\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(response_body.to_string());
            });

            server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getBlockByNumber\"")
                    .body_contains("\"0x1\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(block_response);
            });

            server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getBlockByNumber\"")
                    .body_contains("\"0x2\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(block_response_two);
            });

            let addr = Address::from_str("0x0000000000000000000000000000000000000abc").unwrap();
            let events = fetch_raindex_events(
                &RpcClient::new_with_urls(vec![Url::parse(&server.url("/")).unwrap()]).unwrap(),
                addr,
                1,
                2,
                &FetchConfig {
                    chunk_size: 1000,
                    max_concurrent_requests: 1,
                    max_concurrent_blocks: 1,
                    max_retry_attempts: 1,
                    retry_delay_ms: 0,
                    rate_limit_delay_ms: 0,
                },
            )
            .await
            .unwrap();

            assert_eq!(events.len(), 2);
            assert_eq!(events[0].block_number, U256::from(1));
            assert_eq!(events[1].block_number, U256::from(2));
            assert_eq!(events[0].block_timestamp, Some(U256::from(0x64)));
            assert_eq!(events[1].block_timestamp, Some(U256::from(0x65)));
        }

        #[tokio::test]
        async fn fetch_store_events_returns_empty_for_no_addresses() {
            let addresses: Vec<Address> = vec![];
            let events = fetch_store_events(
                &RpcClient::mock(),
                &addresses,
                0,
                10,
                &FetchConfig::default(),
            )
            .await
            .unwrap();
            assert!(events.is_empty());
        }

        #[tokio::test]
        async fn fetch_store_events_handles_duplicates_and_sorts() {
            let server = MockServer::start();

            let logs_response = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": [
                    {
                        "address": "0x0000000000000000000000000000000000000abc",
                        "topics": [Set::SIGNATURE_HASH.to_string()],
                        "data": "0xdeadbeef",
                        "blockNumber": "0x2",
                        "blockTimestamp": "0x65",
                        "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000002",
                        "transactionIndex": "0x0",
                        "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000002",
                        "logIndex": "0x01",
                        "removed": false
                    },
                    {
                        "address": "0x0000000000000000000000000000000000000abc",
                        "topics": [Set::SIGNATURE_HASH.to_string()],
                        "data": "0xdeadbeef",
                        "blockNumber": "0x1",
                        "blockTimestamp": "0x64",
                        "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
                        "transactionIndex": "0x0",
                        "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
                        "logIndex": "0x00",
                        "removed": false
                    }
                ]
            });

            let addr = Address::from_str("0x0000000000000000000000000000000000000aBc").unwrap();
            let expected_addr = format!("{:#x}", addr);
            let log_mock = server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getLogs\"")
                    .body_contains(&expected_addr);
                then.status(200)
                    .header("content-type", "application/json")
                    .body(logs_response.to_string());
            });

            let addresses = vec![addr, addr, addr];
            let events = fetch_store_events(
                &RpcClient::new_with_urls(vec![Url::parse(&server.url("/")).unwrap()]).unwrap(),
                &addresses,
                1,
                2,
                &FetchConfig {
                    chunk_size: 1000,
                    max_concurrent_requests: 1,
                    max_concurrent_blocks: 1,
                    max_retry_attempts: 1,
                    retry_delay_ms: 0,
                    rate_limit_delay_ms: 0,
                },
            )
            .await
            .unwrap();

            assert_eq!(events.len(), 2);
            assert_eq!(events[0].block_number, U256::from(1));
            assert_eq!(events[0].log_index, U256::from(0));
            assert_eq!(events[1].block_number, U256::from(2));
            assert_eq!(events[1].log_index, U256::from(1));
            assert_eq!(events[0].block_timestamp, Some(U256::from(0x64)));
            assert_eq!(events[1].block_timestamp, Some(U256::from(0x65)));
            assert_eq!(log_mock.hits(), 1);
        }

        #[tokio::test]
        async fn fetch_store_events_returns_error_for_inverted_range() {
            let addr = Address::from_str("0x0000000000000000000000000000000000000abc").unwrap();
            let err =
                fetch_store_events(&RpcClient::mock(), &[addr], 10, 1, &FetchConfig::default())
                    .await
                    .unwrap_err();
            match err {
                LocalDbError::FilterBlockError(FilterBlockError::FromBlockGreaterThanToBlock {
                    from,
                    to,
                }) => {
                    assert_eq!(from, 10);
                    assert_eq!(to, 1);
                }
                other => panic!("expected InvalidBlockRange, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn backfill_missing_timestamps_adds_missing_blocks() {
            let server = MockServer::start();

            server.mock(|when, then| {
                when.method(POST).path("/").json_body(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_getBlockByNumber",
                    "params": ["0x1", false]
                }));
                then.status(200)
                    .header("content-type", "application/json")
                    .body(sample_block_response("0x1", Some("0x10")));
            });

            let mut events = vec![
                make_log_entry_basic("0x1", None),
                make_log_entry_basic("0x2", Some("0x20")),
            ];

            backfill_missing_timestamps(
                &RpcClient::new_with_urls(vec![Url::parse(&server.url("/")).unwrap()]).unwrap(),
                &mut events,
                &FetchConfig::default(),
            )
            .await
            .unwrap();

            assert_eq!(events[0].block_timestamp, Some(U256::from(0x10)));
            assert_eq!(events[1].block_timestamp, Some(U256::from(0x20)));
        }

        #[tokio::test]
        async fn fetch_raindex_events_returns_error_for_inverted_range() {
            let addr = Address::from_str("0x0000000000000000000000000000000000000abc").unwrap();
            let err =
                fetch_raindex_events(&RpcClient::mock(), addr, 10, 1, &FetchConfig::default())
                    .await
                    .unwrap_err();
            match err {
                LocalDbError::FilterBlockError(FilterBlockError::FromBlockGreaterThanToBlock {
                    from,
                    to,
                }) => {
                    assert_eq!(from, 10);
                    assert_eq!(to, 1);
                }
                other => panic!("expected InvalidBlockRange, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn fetch_raindex_events_sorts_numeric_log_index_within_block() {
            let server = MockServer::start();

            let response_body = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": [
                    {
                        "address": "0x0000000000000000000000000000000000000abc",
                        "topics": [
                            format!("0x{}", hex::encode(AddOrderV3::SIGNATURE_HASH))
                        ],
                        "data": "0xdeadbeef",
                        "blockNumber": "0x1",
                        "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
                        "transactionIndex": "0x0",
                        "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
                        "logIndex": "0x10",
                        "removed": false
                    },
                    {
                        "address": "0x0000000000000000000000000000000000000abc",
                        "topics": [
                            format!("0x{}", hex::encode(AddOrderV3::SIGNATURE_HASH))
                        ],
                        "data": "0xdeadbeef",
                        "blockNumber": "0x1",
                        "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000002",
                        "transactionIndex": "0x0",
                        "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000002",
                        "logIndex": "0x02",
                        "removed": false
                    },
                    {
                        "address": "0x0000000000000000000000000000000000000abc",
                        "topics": [
                            format!("0x{}", hex::encode(AddOrderV3::SIGNATURE_HASH))
                        ],
                        "data": "0xdeadbeef",
                        "blockNumber": "0x1",
                        "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000003",
                        "transactionIndex": "0x0",
                        "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000003",
                        "logIndex": "0x0a",
                        "removed": false
                    }
                ]
            });

            // Mock eth_getLogs
            server.mock(|when, then| {
                when.method(POST).path("/").body_contains("\"eth_getLogs\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(response_body.to_string());
            });

            // Backfill timestamp for block 0x1
            server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getBlockByNumber\"")
                    .body_contains("\"0x1\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(sample_block_response("0x1", Some("0x64")));
            });

            let addr = Address::from_str("0x0000000000000000000000000000000000000abc").unwrap();

            let events = fetch_raindex_events(
                &RpcClient::new_with_urls(vec![Url::parse(&server.url("/")).unwrap()]).unwrap(),
                addr,
                1,
                1,
                &FetchConfig {
                    chunk_size: 1000,
                    max_concurrent_requests: 1,
                    max_concurrent_blocks: 1,
                    max_retry_attempts: 1,
                    retry_delay_ms: 0,
                    rate_limit_delay_ms: 0,
                },
            )
            .await
            .unwrap();

            assert_eq!(events.len(), 3);
            // Expect numeric sort of logIndex within the same block: 0x2, 0xA, 0x10
            assert_eq!(events[0].log_index, U256::from(0x2));
            assert_eq!(events[1].log_index, U256::from(0xA));
            assert_eq!(events[2].log_index, U256::from(0x10));
            assert!(events.iter().all(|e| e.block_number == U256::from(1)));
        }

        #[tokio::test]
        async fn retry_helper_returns_rpc_error_after_exhaustion() {
            let attempts = AtomicUsize::new(0);
            let err = retry_with_backoff(
                || async {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>(map_rpc_error(RpcClientError::RpcError {
                        message: "always fail".into(),
                    }))
                },
                2,
                0,
                0,
                should_retry_local_db_error,
                is_rate_limited_local_db_error,
            )
            .await
            .unwrap_err();

            let err: LocalDbError = err.into();
            match err {
                LocalDbError::Rpc(RpcClientError::RpcError { .. }) => {}
                other => panic!("expected LocalDbError::Rpc, got {other:?}"),
            }
            assert_eq!(attempts.load(Ordering::SeqCst), 2, "should attempt twice");
        }

        #[test]
        fn build_store_filters_chunking_basic() {
            let filter = Filter::new()
                .address(vec![Address::ZERO])
                .from_block(1)
                .to_block(10)
                .event_signature(STORE_SET_TOPICS.to_vec());

            let filters = build_log_filters(
                &filter,
                &FetchConfig {
                    chunk_size: 3,
                    ..Default::default()
                },
            )
            .unwrap();

            assert_eq!(filters.len(), 4);
            assert_eq!(filters[0].get_from_block().unwrap(), 1);
            assert_eq!(filters[0].get_to_block().unwrap(), 3);
            assert_eq!(filters[1].get_from_block().unwrap(), 4);
            assert_eq!(filters[1].get_to_block().unwrap(), 6);
            assert_eq!(filters[2].get_from_block().unwrap(), 7);
            assert_eq!(filters[2].get_to_block().unwrap(), 9);
            assert_eq!(filters[3].get_from_block().unwrap(), 10);
            assert_eq!(filters[3].get_to_block().unwrap(), 10);
        }

        #[test]
        fn build_store_filters_handles_u64_max_boundary() {
            let start = u64::MAX - 50;
            let end = u64::MAX;

            let filter = Filter::new()
                .address(vec![Address::ZERO])
                .from_block(start)
                .to_block(end)
                .event_signature(STORE_SET_TOPICS.to_vec());

            let filters = build_log_filters(
                &filter,
                &FetchConfig {
                    chunk_size: 100,
                    ..Default::default()
                },
            )
            .unwrap();

            // Should create a single job and not overflow or loop
            assert_eq!(filters.len(), 1);
            assert_eq!(filters[0].get_from_block().unwrap(), start);
            assert_eq!(filters[0].get_to_block().unwrap(), end);
        }

        #[tokio::test]
        async fn fetch_block_timestamps_empty_input_returns_empty_map() {
            let map = fetch_block_timestamps(&RpcClient::mock(), vec![], &FetchConfig::default())
                .await
                .unwrap();
            assert!(map.is_empty());
        }

        #[tokio::test]
        async fn fetch_block_timestamps_missing_result_field() {
            let server = MockServer::start();

            server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getBlockByNumber\"")
                    .body_contains("\"0x1\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(json!({"jsonrpc":"2.0","id":1,"result":null}).to_string());
            });

            let err = fetch_block_timestamps(
                &RpcClient::new_with_urls(vec![Url::parse(&server.url("/")).unwrap()]).unwrap(),
                vec![1],
                &FetchConfig::new(
                    FetchConfig::DEFAULT_CHUNK_SIZE,
                    FetchConfig::DEFAULT_MAX_CONCURRENT_REQUESTS,
                    FetchConfig::DEFAULT_MAX_CONCURRENT_BLOCKS,
                    1,
                    0,
                    0,
                )
                .expect("fetch config parameters to be valid"),
            )
            .await
            .unwrap_err();

            match err {
                LocalDbError::MissingField { field } => assert_eq!(field, "result"),
                other => panic!("expected MissingField('result'), got {other:?}"),
            }
        }

        #[tokio::test]
        async fn fetch_block_timestamps_missing_timestamp_maps_to_json_error() {
            let server = MockServer::start();

            // Respond with a block object missing the required `timestamp` field
            server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getBlockByNumber\"")
                    .body_contains("\"0x1\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(sample_block_response("0x1", None));
            });

            let err = fetch_block_timestamps(
                &RpcClient::new_with_urls(vec![Url::parse(&server.url("/")).unwrap()]).unwrap(),
                vec![1],
                &FetchConfig::new(
                    FetchConfig::DEFAULT_CHUNK_SIZE,
                    FetchConfig::DEFAULT_MAX_CONCURRENT_REQUESTS,
                    FetchConfig::DEFAULT_MAX_CONCURRENT_BLOCKS,
                    1,
                    0,
                    0,
                )
                .expect("fetch config parameters to be valid"),
            )
            .await
            .unwrap_err();

            match err {
                LocalDbError::JsonParse(_) => {}
                other => panic!("expected JsonParse, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn fetch_store_events_chunking_multiple_filters_merged_and_sorted() {
            let server = MockServer::start();

            // Job 1: from 0x1 to 0x1
            server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getLogs\"")
                    .body_contains("\"0x1\"")
                    .body_contains("\"0x1\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(
                        json!({
                            "jsonrpc":"2.0",
                            "id":1,
                            "result":[{
                                "address":"0x0000000000000000000000000000000000000abc",
                                "topics":[Set::SIGNATURE_HASH.to_string()],
                                "data":"0xdeadbeef",
                                "blockNumber":"0x1",
                                "blockTimestamp":"0x64",
                                "transactionHash":"0x0000000000000000000000000000000000000000000000000000000000000001",
                                "transactionIndex":"0x0",
                                "blockHash":"0x0000000000000000000000000000000000000000000000000000000000000001",
                                "logIndex":"0x01",
                                "removed":false
                            }]
                        })
                        .to_string(),
                    );
            });

            // Job 2: from 0x2 to 0x2
            server.mock(|when, then| {
                when.method(POST)
                    .path("/")
                    .body_contains("\"eth_getLogs\"")
                    .body_contains("\"0x2\"")
                    .body_contains("\"0x2\"");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(
                        json!({
                            "jsonrpc":"2.0",
                            "id":1,
                            "result":[{
                                "address":"0x0000000000000000000000000000000000000abc",
                                "topics":[Set::SIGNATURE_HASH.to_string()],
                                "data":"0xdeadbeef",
                                "blockNumber":"0x2",
                                "blockTimestamp":"0x65",
                                "transactionHash":"0x0000000000000000000000000000000000000000000000000000000000000002",
                                "transactionIndex":"0x0",
                                "blockHash":"0x0000000000000000000000000000000000000000000000000000000000000002",
                                "logIndex":"0x00",
                                "removed":false
                            }]
                        })
                        .to_string(),
                    );
            });

            let addr = Address::from_str("0x0000000000000000000000000000000000000abc").unwrap();

            let events = fetch_store_events(
                &RpcClient::new_with_urls(vec![Url::parse(&server.url("/")).unwrap()]).unwrap(),
                &[addr],
                1,
                2,
                &FetchConfig {
                    chunk_size: 1,
                    max_concurrent_requests: 2,
                    max_concurrent_blocks: 1,
                    max_retry_attempts: 1,
                    retry_delay_ms: 0,
                    rate_limit_delay_ms: 0,
                },
            )
            .await
            .unwrap();

            assert_eq!(events.len(), 2);
            assert_eq!(events[0].block_number, U256::from(1));
            assert_eq!(events[0].log_index, U256::from(1));
            assert_eq!(events[1].block_number, U256::from(2));
            assert_eq!(events[1].log_index, U256::ZERO);
            assert_eq!(events[0].block_timestamp, Some(U256::from(0x64)));
            assert_eq!(events[1].block_timestamp, Some(U256::from(0x65)));
        }
    }
}
