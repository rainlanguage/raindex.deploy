use alloy::primitives::{Address, B256};
use async_trait::async_trait;
use url::Url;

use crate::local_db::decode::{decode_events, DecodedEvent, DecodedEventData};
use crate::local_db::fetch::{fetch_raindex_events, fetch_store_events};
use crate::local_db::pipeline::EventsPipeline;
use crate::local_db::{FetchConfig, LocalDbError};
use crate::rpc_client::{LogEntryResponse, RpcClient};

/// Shared implementation of the EventsPipeline that delegates to LocalDb.
///
/// Construction determines the backend policy:
/// - `with_regular_rpcs` for browser/public RPCs
/// - `with_hyperrpc` for producer/HyperRPC
/// - `from_local_db` if the runner builds a LocalDb externally
#[derive(Debug, Clone)]
pub struct DefaultEventsPipeline {
    rpc_client: RpcClient,
}

impl DefaultEventsPipeline {
    /// Constructs the pipeline using regular/public RPC URLs.
    pub fn with_regular_rpcs(rpcs: Vec<Url>) -> Result<Self, LocalDbError> {
        let rpc_client = RpcClient::new_with_urls(rpcs)?;
        Ok(Self { rpc_client })
    }

    /// Constructs the pipeline using HyperRPC (producer path).
    pub fn with_hyperrpc(chain_id: u32, api_token: String) -> Result<Self, LocalDbError> {
        let rpc_client = RpcClient::new_with_hyper_rpc(chain_id, &api_token)?;
        Ok(Self { rpc_client })
    }
}

#[async_trait(?Send)]
impl EventsPipeline for DefaultEventsPipeline {
    async fn latest_block(&self) -> Result<u64, LocalDbError> {
        self.rpc_client
            .get_latest_block_number()
            .await
            .map_err(Into::into)
    }

    async fn block_hash(&self, block_number: u64) -> Result<B256, LocalDbError> {
        let block = self
            .rpc_client
            .get_block_by_number(block_number)
            .await?
            .ok_or_else(|| LocalDbError::BlockHashNotFound { block_number })?;
        Ok(block.hash)
    }

    async fn fetch_raindex(
        &self,
        raindex_address: Address,
        from_block: u64,
        to_block: u64,
        cfg: &FetchConfig,
    ) -> Result<Vec<LogEntryResponse>, LocalDbError> {
        fetch_raindex_events(&self.rpc_client, raindex_address, from_block, to_block, cfg).await
    }

    async fn fetch_stores(
        &self,
        store_addresses: &[Address],
        from_block: u64,
        to_block: u64,
        cfg: &FetchConfig,
    ) -> Result<Vec<LogEntryResponse>, LocalDbError> {
        fetch_store_events(&self.rpc_client, store_addresses, from_block, to_block, cfg).await
    }

    fn decode(
        &self,
        logs: &[LogEntryResponse],
    ) -> Result<Vec<DecodedEventData<DecodedEvent>>, LocalDbError> {
        decode_events(logs).map_err(LocalDbError::DecodeError)
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use crate::rpc_client::RpcClientError;
    use alloy::primitives::{b256, Bytes, U256};
    use alloy::sol_types::SolEvent;
    use httpmock::MockServer;
    use raindex_bindings::Raindex::MetaV1_2;
    use serde_json::json;
    use std::str::FromStr;

    fn test_url() -> Url {
        Url::parse("http://localhost:8545").expect("valid test url")
    }

    #[test]
    fn constructors_build_ok() {
        // with_regular_rpcs
        let pipe = DefaultEventsPipeline::with_regular_rpcs(vec![test_url()])
            .expect("build with regular rpcs");

        // with_hyperrpc (uses the numeric chain ID alias; token string is arbitrary)
        let _pipe3 = DefaultEventsPipeline::with_hyperrpc(42161, "token".to_string())
            .expect("build with hyperrpc");
        drop(pipe);
    }

    #[test]
    fn decode_propagates_decode_errors() {
        let pipe = DefaultEventsPipeline::with_regular_rpcs(vec![test_url()]).unwrap();

        // Valid topic but empty data triggers a decode error path.
        let bad_log = LogEntryResponse {
            address: Address::ZERO,
            topics: vec![Bytes::from(MetaV1_2::SIGNATURE_HASH.as_slice().to_vec())],
            data: Bytes::new(),
            block_number: U256::from(1),
            block_timestamp: Some(U256::from(2)),
            transaction_hash: b256!(
                "0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
            ),
            transaction_index: "0x0".to_string(),
            block_hash: b256!("0xbbccddeeff00112233445566778899aabbccddeeff00112233445566778899aa"),
            log_index: U256::ZERO,
            removed: false,
        };

        let err = pipe.decode(&[bad_log]).expect_err("expected decode error");
        match err {
            LocalDbError::DecodeError(_) => {}
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn with_regular_rpcs_rejects_empty_list() {
        let err = DefaultEventsPipeline::with_regular_rpcs(vec![]).expect_err("expected error");
        match err {
            LocalDbError::Rpc(RpcClientError::Config { .. }) => {}
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn block_hash_conversion_preserves_polygon_hash() {
        let server = MockServer::start();
        let polygon_hash = "0xfeedfeedfeedfeedfeedfeedfeedfeedfeedfeedfeedfeedfeedfeedfeedfeed";

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
                .body(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": {
                            "timestamp": "0x64b8c123",
                            "hash": polygon_hash,
                            "totalDifficulty": "0x2"
                        }
                    })
                    .to_string(),
                );
        });

        let mut pipeline =
            DefaultEventsPipeline::with_hyperrpc(137, "token".to_string()).expect("valid pipeline");
        pipeline.rpc_client.update_rpc_urls(vec![
            Url::parse(&server.base_url()).expect("valid server url")
        ]);

        let block_hash = pipeline
            .block_hash(100)
            .await
            .expect("block hash should deserialize");
        let expected = B256::from_str(polygon_hash).expect("polygon hash should parse");
        assert_eq!(block_hash, expected);

        mock.assert();
    }
}
