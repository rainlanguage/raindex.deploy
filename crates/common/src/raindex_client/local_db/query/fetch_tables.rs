use crate::local_db::query::fetch_tables::{fetch_tables_stmt, TableResponse};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};

pub async fn fetch_all_tables<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
) -> Result<Vec<TableResponse>, LocalDbQueryError> {
    exec.query_json_retryable(&fetch_tables_stmt()).await
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn wrapper_uses_raw_sql_exactly() {
        let expected_stmt = fetch_tables_stmt();
        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_all_tables(&exec).await;
        assert!(res.is_ok());

        let captured = store.borrow().clone();
        assert_eq!(captured.0, expected_stmt.sql);
    }

    #[wasm_bindgen_test]
    async fn invalid_response_yields_invalid_response_error() {
        // Return a raw JsValue string instead of WasmEncodedResult
        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let store_clone = store.clone();
        let closure = Closure::wrap(Box::new(
            move |sql: String, params: wasm_bindgen::JsValue| -> wasm_bindgen::JsValue {
                *store_clone.borrow_mut() = (sql, params);
                wasm_bindgen::JsValue::from_str("not-a-wrapper")
            },
        )
            as Box<dyn FnMut(String, wasm_bindgen::JsValue) -> wasm_bindgen::JsValue>);
        let callback: js_sys::Function = closure.as_ref().clone().unchecked_into();
        closure.forget();

        let exec = JsCallbackExecutor::from_ref(&callback);
        let res = super::fetch_all_tables(&exec).await;
        assert!(matches!(res, Err(LocalDbQueryError::InvalidResponse)));
    }
}
