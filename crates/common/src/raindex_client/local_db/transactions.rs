use super::super::transactions::RaindexTransaction;
use super::super::RaindexError;
use super::LocalDb;
use crate::local_db::query::fetch_transaction_by_hash::{
    build_fetch_transaction_by_hash_stmt, LocalDbTransaction,
};
use crate::local_db::query::LocalDbQueryExecutor;
use crate::local_db::RaindexIdentifier;
use alloy::primitives::B256;

pub struct LocalDbTransactions<'a> {
    pub(crate) db: &'a LocalDb,
}

impl<'a> LocalDbTransactions<'a> {
    pub(crate) fn new(db: &'a LocalDb) -> Self {
        Self { db }
    }

    /// Fetch transaction info by transaction hash from the local DB.
    /// Returns None if no transaction with that hash is found.
    pub async fn get_by_tx_hash(
        &self,
        raindex_id: &RaindexIdentifier,
        tx_hash: B256,
    ) -> Result<Option<RaindexTransaction>, RaindexError> {
        let stmt = build_fetch_transaction_by_hash_stmt(raindex_id, tx_hash);
        let results: Vec<LocalDbTransaction> = self.db.query_json_retryable(&stmt).await?;

        if let Some(local_tx) = results.into_iter().next() {
            let tx = RaindexTransaction::from_local_parts(
                local_tx.transaction_hash,
                local_tx.sender,
                local_tx.block_number,
                local_tx.block_timestamp,
            )?;
            return Ok(Some(tx));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_family = "wasm")]
    use super::*;

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use super::*;
        use crate::raindex_client::local_db::executor::JsCallbackExecutor;
        use crate::raindex_client::local_db::LocalDb;
        use alloy::primitives::{address, b256};
        use serde_json::json;
        use wasm_bindgen_test::wasm_bindgen_test;
        use wasm_bindgen_utils::prelude::*;

        fn create_mock_callback(response_json: &str) -> js_sys::Function {
            let json_str = response_json.to_string();
            let result = WasmEncodedResult::Success::<String> {
                value: json_str,
                error: None,
            };
            let payload = js_sys::JSON::stringify(&serde_wasm_bindgen::to_value(&result).unwrap())
                .unwrap()
                .as_string()
                .unwrap();

            let closure =
                Closure::wrap(Box::new(move |_sql: String, _params: JsValue| -> JsValue {
                    js_sys::JSON::parse(&payload).unwrap()
                })
                    as Box<dyn Fn(String, JsValue) -> JsValue>);

            closure.into_js_value().dyn_into().unwrap()
        }

        #[wasm_bindgen_test]
        async fn test_get_by_tx_hash_returns_transaction_when_found() {
            let tx_hash =
                b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890");
            let sender = address!("0x1111111111111111111111111111111111111111");
            let raindex_addr = address!("0x2222222222222222222222222222222222222222");

            let tx_json = json!([{
                "transactionHash": tx_hash.to_string(),
                "blockNumber": 12345,
                "blockTimestamp": 1700000000,
                "sender": sender.to_string()
            }]);

            let callback = create_mock_callback(&tx_json.to_string());
            let exec = JsCallbackExecutor::from_ref(&callback);
            let local_db = LocalDb::new(exec);

            let transactions = LocalDbTransactions::new(&local_db);
            let raindex_id = RaindexIdentifier::new(1, raindex_addr);

            let result = transactions.get_by_tx_hash(&raindex_id, tx_hash).await;

            assert!(result.is_ok());
            let tx = result.unwrap();
            assert!(tx.is_some());
            let tx = tx.unwrap();
            assert_eq!(tx.id().to_lowercase(), tx_hash.to_string().to_lowercase());
        }

        #[wasm_bindgen_test]
        async fn test_get_by_tx_hash_returns_none_when_not_found() {
            let tx_hash =
                b256!("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890");
            let raindex_addr = address!("0x2222222222222222222222222222222222222222");

            let callback = create_mock_callback("[]");
            let exec = JsCallbackExecutor::from_ref(&callback);
            let local_db = LocalDb::new(exec);

            let transactions = LocalDbTransactions::new(&local_db);
            let raindex_id = RaindexIdentifier::new(1, raindex_addr);

            let result = transactions.get_by_tx_hash(&raindex_id, tx_hash).await;

            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
        }
    }
}
