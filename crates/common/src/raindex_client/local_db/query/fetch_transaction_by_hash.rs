use crate::local_db::query::fetch_transaction_by_hash::{
    build_fetch_transaction_by_hash_stmt, LocalDbTransaction,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::local_db::RaindexIdentifier;
use alloy::primitives::B256;

pub async fn fetch_transaction_by_hash<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
    tx_hash: B256,
) -> Result<Vec<LocalDbTransaction>, LocalDbQueryError> {
    let stmt = build_fetch_transaction_by_hash_stmt(raindex_id, tx_hash);
    exec.query_json_retryable(&stmt).await
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use alloy::primitives::{address, b256, Address};
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_exactly() {
        let tx_hash = b256!("0x0000000000000000000000000000000000000000000000000000000000000abc");
        let raindex = Address::from([0x51; 20]);
        let expected_stmt =
            build_fetch_transaction_by_hash_stmt(&RaindexIdentifier::new(1, raindex), tx_hash);

        let store = Rc::new(RefCell::new((String::new(), JsValue::UNDEFINED)));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res =
            super::fetch_transaction_by_hash(&exec, &RaindexIdentifier::new(1, raindex), tx_hash)
                .await;
        assert!(res.is_ok());
        assert_eq!(store.borrow().clone().0, expected_stmt.sql);
    }

    #[wasm_bindgen_test]
    async fn wrapper_returns_rows_when_present() {
        let tx_hash = b256!("0x0000000000000000000000000000000000000000000000000000000000000abc");
        let raindex = address!("0x5151515151515151515151515151515151515151");
        let sender = address!("0x1111111111111111111111111111111111111111");
        let expected_stmt =
            build_fetch_transaction_by_hash_stmt(&RaindexIdentifier::new(1, raindex), tx_hash);

        let row_json = format!(
            r#"[{{
                "transactionHash":"{}",
                "blockNumber":100,
                "blockTimestamp":999,
                "sender":"{}"
            }}]"#,
            tx_hash, sender
        );

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback(&row_json, store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res =
            super::fetch_transaction_by_hash(&exec, &RaindexIdentifier::new(1, raindex), tx_hash)
                .await;
        assert!(res.is_ok());
        let rows = res.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(store.borrow().clone().0, expected_stmt.sql);
    }
}
