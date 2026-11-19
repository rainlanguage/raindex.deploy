use crate::erc20::TokenInfo;
use crate::local_db::pipeline::TokensPipeline;
use crate::local_db::query::fetch_erc20_tokens_by_addresses::{build_fetch_stmt, Erc20TokenRow};
use crate::local_db::query::LocalDbQueryExecutor;
use crate::local_db::token_fetch::fetch_erc20_metadata_concurrent;
use crate::local_db::{FetchConfig, LocalDbError, RaindexIdentifier};
use crate::rpc_client::RpcClient;
use alloy::primitives::Address;
use async_trait::async_trait;
use url::Url;

/// Default implementation of the TokensPipeline that delegates to the
/// existing concurrent ERC-20 metadata fetcher and query builders.
#[derive(Debug, Clone)]
pub struct DefaultTokensPipeline {
    rpc_client: RpcClient,
}

impl DefaultTokensPipeline {
    pub fn new(urls: Vec<Url>) -> Result<Self, LocalDbError> {
        let rpc_client = RpcClient::new_with_urls(urls)?;
        Ok(Self { rpc_client })
    }

    #[cfg(test)]
    pub fn mock() -> Self {
        let rpc_client =
            RpcClient::new_with_urls(vec![Url::parse("https://test.com").unwrap()]).unwrap();
        Self { rpc_client }
    }
}

#[async_trait(?Send)]
impl TokensPipeline for DefaultTokensPipeline {
    async fn load_existing<DB>(
        &self,
        db: &DB,
        raindex_id: &RaindexIdentifier,
        token_addrs_lower: &[Address],
    ) -> Result<Vec<Erc20TokenRow>, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let Some(stmt) = build_fetch_stmt(raindex_id, token_addrs_lower)? else {
            return Ok(vec![]);
        };
        let rows: Vec<Erc20TokenRow> = db.query_json_retryable(&stmt).await?;
        Ok(rows)
    }

    async fn fetch_missing(
        &self,
        missing: Vec<Address>,
        cfg: &FetchConfig,
    ) -> Result<Vec<(Address, TokenInfo)>, LocalDbError> {
        fetch_erc20_metadata_concurrent(&self.rpc_client, missing, cfg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::query::{LocalDbQueryError, SqlStatement, SqlStatementBatch};
    use alloy::hex;
    use serde_json::json;

    struct MockDb {
        // Preloaded rows to return for query_json
        rows: Vec<Erc20TokenRow>,
        // Assertions for the incoming statement
        expect_in_clause: bool,
        expect_chain_id: Option<u32>,
        expect_raindex: Option<String>,
        expect_addr_count: Option<usize>,
        // Optional explicit expected address parameter values (in order)
        expect_addr_values: Option<Vec<String>>,
        // Inject a DB error from query_json when set
        fail_query: bool,
        // Force a deserialization error from query_json when set
        return_malformed_json: bool,
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for MockDb {
        async fn execute_batch(&self, _batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            Ok(())
        }

        async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: crate::local_db::query::FromDbJson,
        {
            if self.fail_query {
                return Err(LocalDbQueryError::database("boom"));
            }
            if self.return_malformed_json {
                // Return a shape that won't deserialize to the expected T
                let bad = json!({"unexpected": "shape"});
                return serde_json::from_value::<T>(bad)
                    .map_err(|e| LocalDbQueryError::deserialization(e.to_string()));
            }
            if let Some(cid) = self.expect_chain_id {
                match stmt.params().first() {
                    Some(crate::local_db::query::SqlValue::U64(v)) => assert_eq!(*v, cid as u64),
                    other => panic!("expected first param U64({cid}), got {other:?}"),
                }
            }
            if let Some(expected_raindex) = self.expect_raindex.as_ref() {
                match stmt.params().get(1) {
                    Some(crate::local_db::query::SqlValue::Text(actual)) => {
                        assert_eq!(actual, &expected_raindex.to_ascii_lowercase());
                    }
                    other => {
                        panic!("expected second param Text({expected_raindex}), got {other:?}")
                    }
                }
            }
            if let Some(n) = self.expect_addr_count {
                // There should be chain id + raindex + n address params
                assert_eq!(stmt.params().len(), 2 + n);
            }
            if let Some(expected) = &self.expect_addr_values {
                let addr_params: Vec<String> = stmt
                    .params()
                    .iter()
                    .skip(2)
                    .map(|v| match v {
                        crate::local_db::query::SqlValue::Text(s) => s.clone(),
                        other => panic!("expected Text param for address, got {other:?}"),
                    })
                    .collect();
                assert_eq!(
                    &addr_params, expected,
                    "address params must match input order"
                );
            }
            if self.expect_in_clause {
                assert!(stmt.sql().contains("IN ("), "expected IN clause in SQL");
            }
            let v = serde_json::to_value(&self.rows)
                .map_err(|e| LocalDbQueryError::deserialization(e.to_string()))?;
            serde_json::from_value::<T>(json!(v))
                .map_err(|e| LocalDbQueryError::deserialization(e.to_string()))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Ok(String::new())
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    #[tokio::test]
    async fn load_existing_empty_addresses_short_circuits() {
        let raindex = Address::from([0xbe; 20]);
        let db = MockDb {
            rows: vec![],
            expect_in_clause: false,
            expect_chain_id: None,
            expect_raindex: None,
            expect_addr_count: None,
            expect_addr_values: None,
            fail_query: false,
            return_malformed_json: false,
        };
        let pipeline = DefaultTokensPipeline::mock();
        let out = pipeline
            .load_existing(&db, &RaindexIdentifier::new(137, raindex), &[])
            .await
            .expect("ok");
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn load_existing_builds_query_and_returns_rows() {
        let raindex = Address::from([0xab; 20]);
        let token_addr = Address::from([0xac; 20]);
        let other_addr = Address::from([0xad; 20]);
        let row = Erc20TokenRow {
            chain_id: 137,
            raindex_address: raindex,
            token_address: token_addr,
            name: "Token".to_string(),
            symbol: "TKN".to_string(),
            decimals: 18,
        };
        let db = MockDb {
            rows: vec![row.clone()],
            expect_in_clause: true,
            expect_chain_id: Some(137),
            expect_raindex: Some(raindex.to_string()),
            expect_addr_count: Some(2),
            expect_addr_values: Some(vec![
                hex::encode_prefixed(token_addr),
                hex::encode_prefixed(other_addr),
            ]),
            fail_query: false,
            return_malformed_json: false,
        };
        let pipeline = DefaultTokensPipeline::mock();
        let addrs = vec![token_addr, other_addr];
        let out = pipeline
            .load_existing(&db, &RaindexIdentifier::new(137, raindex), &addrs)
            .await
            .expect("ok");
        assert_eq!(out, vec![row]);
    }

    #[tokio::test]
    async fn load_existing_propagates_db_error() {
        let raindex = Address::from([0xcd; 20]);
        let token_addr = Address::from([0xae; 20]);
        let db = MockDb {
            rows: vec![],
            expect_in_clause: true,
            expect_chain_id: Some(1),
            expect_raindex: Some(raindex.to_string()),
            expect_addr_count: Some(1),
            expect_addr_values: Some(vec![hex::encode_prefixed(token_addr)]),
            fail_query: true,
            return_malformed_json: false,
        };
        let pipeline = DefaultTokensPipeline::mock();
        let err = pipeline
            .load_existing(&db, &RaindexIdentifier::new(1, raindex), &[token_addr])
            .await
            .expect_err("should fail");
        match err {
            LocalDbError::LocalDbQueryError(e) => match e {
                crate::local_db::query::LocalDbQueryError::Database { .. } => {}
                other => panic!("expected Database error, got {other:?}"),
            },
            other => panic!("expected LocalDbQueryError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn load_existing_deserialization_error_bubbles() {
        let raindex = Address::from([0xde; 20]);
        let token_addr = Address::from([0xaf; 20]);
        let db = MockDb {
            rows: vec![],
            expect_in_clause: true,
            expect_chain_id: Some(1),
            expect_raindex: Some(raindex.to_string()),
            expect_addr_count: Some(1),
            expect_addr_values: Some(vec![hex::encode_prefixed(token_addr)]),
            fail_query: false,
            return_malformed_json: true,
        };
        let pipeline = DefaultTokensPipeline::mock();
        let err = pipeline
            .load_existing(&db, &RaindexIdentifier::new(1, raindex), &[token_addr])
            .await
            .expect_err("should fail");
        match err {
            LocalDbError::LocalDbQueryError(e) => match e {
                crate::local_db::query::LocalDbQueryError::Deserialization { .. } => {}
                other => panic!("expected Deserialization error, got {other:?}"),
            },
            other => panic!("expected LocalDbQueryError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn load_existing_keeps_duplicate_addresses_in_order() {
        let raindex = Address::from([0xef; 20]);
        let addr_a = Address::from([0xaa; 20]);
        let addr_b = Address::from([0xbb; 20]);
        let db = MockDb {
            rows: vec![],
            expect_in_clause: true,
            expect_chain_id: Some(137),
            expect_raindex: Some(raindex.to_string()),
            expect_addr_count: Some(3),
            expect_addr_values: Some(vec![
                hex::encode_prefixed(addr_a),
                hex::encode_prefixed(addr_a),
                hex::encode_prefixed(addr_b),
            ]),
            fail_query: false,
            return_malformed_json: false,
        };
        let pipeline = DefaultTokensPipeline::mock();
        let addrs = vec![addr_a, addr_a, addr_b];
        // We only care that params keep duplicates in order; rows are empty
        let out = pipeline
            .load_existing(&db, &RaindexIdentifier::new(137, raindex), &addrs)
            .await
            .expect("ok");
        assert!(out.is_empty());
    }

    #[cfg(not(target_family = "wasm"))]
    mod non_wasm_tests {
        use super::*;
        use alloy::primitives::Address;
        use raindex_test_fixtures::LocalEvm;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn fetch_missing_delegates_to_concurrent_fetcher() {
            let local_evm = LocalEvm::new_with_tokens(1).await;
            let url = Url::parse(&local_evm.url()).unwrap();
            let token = local_evm.tokens[0].clone();
            let addr: Address = *token.address();

            let pipeline = DefaultTokensPipeline::new(vec![url]).unwrap();
            let out = pipeline
                .fetch_missing(vec![addr], &FetchConfig::default())
                .await
                .expect("fetch ok");

            assert_eq!(out.len(), 1);
            assert_eq!(out[0].0, addr);
            assert_eq!(out[0].1.decimals, 18);
            assert_eq!(out[0].1.symbol, "TOKEN1");
            assert_eq!(out[0].1.name, "Token1");
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn fetch_missing_multiple_tokens() {
            let local_evm = LocalEvm::new_with_tokens(2).await;
            let url = Url::parse(&local_evm.url()).unwrap();
            let t0 = local_evm.tokens[0].clone();
            let t1 = local_evm.tokens[1].clone();
            let a0: Address = *t0.address();
            let a1: Address = *t1.address();

            let pipeline = DefaultTokensPipeline::new(vec![url]).unwrap();
            let mut out = pipeline
                .fetch_missing(vec![a0, a1], &FetchConfig::default())
                .await
                .expect("fetch ok");
            out.sort_by_key(|(a, _)| *a);
            let mut expected = vec![a0, a1];
            expected.sort();
            assert_eq!(out.len(), 2);
            assert_eq!(out.iter().map(|(a, _)| *a).collect::<Vec<_>>(), expected);
        }

        #[tokio::test]
        async fn fetch_missing_failure_bubbles_error() {
            let bad_url = Url::parse("http://127.0.0.1:1").unwrap();

            let pipeline = DefaultTokensPipeline::new(vec![bad_url]).unwrap();
            let err = pipeline
                .fetch_missing(vec![Address::ZERO], &FetchConfig::default())
                .await
                .expect_err("expected error");
            match err {
                LocalDbError::ERC20Error(crate::erc20::Error::MulticallError(multicall_err)) => {
                    use alloy::providers::MulticallError;
                    assert!(
                        matches!(multicall_err, MulticallError::TransportError(_)),
                        "expected transport-related failure, got {multicall_err:?}"
                    );
                }
                other => panic!("unexpected error variant: {other:?}"),
            }
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn fetch_missing_mixed_success_and_failure_returns_error() {
            let local_evm = LocalEvm::new_with_tokens(1).await;
            let url = Url::parse(&local_evm.url()).unwrap();
            let good = *local_evm.tokens[0].address();
            let bad = Address::ZERO; // not a valid token contract in the fixture

            let pipeline = DefaultTokensPipeline::new(vec![url]).unwrap();
            let err = pipeline
                .fetch_missing(vec![good, bad], &FetchConfig::default())
                .await
                .expect_err("expected overall error on partial failure");

            match err {
                LocalDbError::ERC20Error(crate::erc20::Error::MulticallError(multicall_err)) => {
                    use alloy::providers::MulticallError;
                    assert!(
                        matches!(multicall_err, MulticallError::DecodeError(_)),
                        "expected decode failure for invalid token address, got {multicall_err:?}"
                    );
                }
                other => panic!("unexpected error variant: {other:?}"),
            }
        }
    }
}
