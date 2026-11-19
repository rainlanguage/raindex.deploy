use crate::local_db::query::clear_tables::{clear_tables_batch, vacuum_stmt};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};

pub async fn clear_tables<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
) -> Result<(), LocalDbQueryError> {
    exec.execute_batch(&clear_tables_batch()).await?;
    exec.query_text(&vacuum_stmt()).await.map(|_| ())
}

#[cfg(target_family = "wasm")]
mod wasm {
    use super::*;
    use crate::local_db::LocalDbError;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use wasm_bindgen_utils::{prelude::*, wasm_export};

    #[wasm_export(js_name = "clearTables", unchecked_return_type = "void")]
    pub async fn clear_tables_wasm(
        #[wasm_export(
            param_description = "Local database object with query, wipeAndRecreate, and transaction functions"
        )]
        local_db: JsValue,
    ) -> Result<(), LocalDbError> {
        let exec = JsCallbackExecutor::new(local_db).map_err(LocalDbError::from)?;
        clear_tables(&exec).await.map_err(LocalDbError::from)
    }
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::{serde_wasm_bindgen, JsValue};
    use wasm_bindgen_utils::result::WasmEncodedResult;
    use web_sys::js_sys::{Array, Function, Reflect};

    #[wasm_bindgen_test]
    async fn wrapper_uses_transaction_then_vacuum() {
        let transaction_calls: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let transaction_calls_clone = transaction_calls.clone();
        let transaction = Closure::wrap(Box::new(move |statements: JsValue| -> JsValue {
            transaction_calls_clone.borrow_mut().push(statements);
            serde_wasm_bindgen::to_value(&WasmEncodedResult::Success::<String> {
                value: String::new(),
                error: None,
            })
            .unwrap()
        }) as Box<dyn FnMut(JsValue) -> JsValue>);

        let query_calls: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let query_calls_clone = query_calls.clone();
        let query = Closure::wrap(Box::new(move |sql: String, _params: JsValue| -> JsValue {
            query_calls_clone.borrow_mut().push(sql);
            serde_wasm_bindgen::to_value(&WasmEncodedResult::Success::<String> {
                value: String::new(),
                error: None,
            })
            .unwrap()
        }) as Box<dyn FnMut(String, JsValue) -> JsValue>);

        let local_db = crate::raindex_client::tests::local_db_object_from_query_callback(
            query.as_ref().unchecked_ref::<Function>().clone(),
        );
        Reflect::set(
            &local_db,
            &JsValue::from_str("transaction"),
            transaction.as_ref().unchecked_ref(),
        )
        .unwrap();
        query.forget();
        transaction.forget();

        let exec = JsCallbackExecutor::new(local_db.into()).unwrap();
        let res = super::clear_tables(&exec).await;
        assert!(res.is_ok());
        assert_eq!(transaction_calls.borrow().len(), 1);
        assert!(Array::from(&transaction_calls.borrow()[0]).length() > 1);
        assert_eq!(query_calls.borrow().as_slice(), [vacuum_stmt().sql()]);
    }
}
