use crate::local_db::query::fetch_store_addresses::{fetch_store_addresses_stmt, StoreAddressRow};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::local_db::RaindexIdentifier;

pub async fn fetch_store_addresses<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
) -> Result<Vec<StoreAddressRow>, LocalDbQueryError> {
    exec.query_json_retryable(&fetch_store_addresses_stmt(raindex_id))
        .await
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use alloy::primitives::Address;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::wasm_bindgen;

    #[wasm_bindgen_test]
    async fn wrapper_uses_raw_sql_exactly() {
        let raindex = Address::from([0x55; 20]);
        let expected_stmt = fetch_store_addresses_stmt(&RaindexIdentifier::new(1, raindex));
        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);
        let res = super::fetch_store_addresses(&exec, &RaindexIdentifier::new(1, raindex)).await;
        assert!(res.is_ok());
        assert_eq!(store.borrow().clone().0, expected_stmt.sql);
    }
}
