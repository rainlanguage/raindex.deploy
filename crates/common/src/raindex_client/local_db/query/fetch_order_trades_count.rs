use alloy::primitives::B256;

use crate::local_db::query::fetch_order_trades_count::{
    build_fetch_trade_count_stmt, extract_trade_count, LocalDbTradeCountRow,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::local_db::RaindexIdentifier;

pub async fn fetch_order_trades_count<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
    order_hash: B256,
    start_timestamp: Option<u64>,
    end_timestamp: Option<u64>,
) -> Result<u64, LocalDbQueryError> {
    let stmt =
        build_fetch_trade_count_stmt(raindex_id, order_hash, start_timestamp, end_timestamp)?;
    let rows: Vec<LocalDbTradeCountRow> = exec.query_json_retryable(&stmt).await?;
    Ok(extract_trade_count(&rows))
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use alloy::primitives::{b256, Address};
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::wasm_bindgen;

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_and_extracts_count() {
        let order_hash =
            b256!("0x000000000000000000000000000000000000000000000000000000000000abcd");
        let start = Some(10);
        let end = Some(20);

        let raindex = Address::from([0x88; 20]);
        let expected_stmt = build_fetch_trade_count_stmt(
            &RaindexIdentifier::new(1, raindex),
            order_hash.clone(),
            start,
            end,
        )
        .unwrap();

        // Return one row with count 5
        let response = r#"[{"trade_count":5}]"#;
        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback(response, store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_order_trades_count(
            &exec,
            &RaindexIdentifier::new(1, raindex),
            order_hash,
            start,
            end,
        )
        .await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 5);

        let captured = store.borrow().clone();
        assert_eq!(captured.0, expected_stmt.sql);
    }

    #[wasm_bindgen_test]
    async fn wrapper_extracts_zero_on_empty_rows() {
        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);
        let res = super::fetch_order_trades_count(
            &exec,
            &RaindexIdentifier::new(1, Address::ZERO),
            b256!("0x00000000000000000000000000000000000000000000000000000000deadbeef"),
            None,
            None,
        )
        .await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 0);
    }
}
