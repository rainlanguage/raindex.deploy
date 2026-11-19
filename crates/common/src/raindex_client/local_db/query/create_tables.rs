use crate::local_db::query::create_tables::create_tables_batch;
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};

pub async fn create_tables<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
) -> Result<(), LocalDbQueryError> {
    exec.execute_batch(&create_tables_batch()).await
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
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
    async fn wrapper_uses_transaction_batch() {
        let calls: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let calls_clone = calls.clone();
        let transaction = Closure::wrap(Box::new(move |statements: JsValue| -> JsValue {
            calls_clone.borrow_mut().push(statements);
            serde_wasm_bindgen::to_value(&WasmEncodedResult::Success::<String> {
                value: String::new(),
                error: None,
            })
            .unwrap()
        }) as Box<dyn FnMut(JsValue) -> JsValue>);

        let local_db = crate::raindex_client::tests::local_db_object_from_query_callback(
            Function::new_no_args("return { value: '', error: null };"),
        );
        Reflect::set(
            &local_db,
            &JsValue::from_str("transaction"),
            transaction.as_ref().unchecked_ref(),
        )
        .unwrap();
        transaction.forget();

        let exec = JsCallbackExecutor::new(local_db.into()).unwrap();
        let res = super::create_tables(&exec).await;
        assert!(res.is_ok());
        assert_eq!(calls.borrow().len(), 1);
        assert!(Array::from(&calls.borrow()[0]).length() > 1);
    }
}
