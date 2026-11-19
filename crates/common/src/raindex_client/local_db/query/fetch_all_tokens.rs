use crate::local_db::query::fetch_all_tokens::{
    build_fetch_all_tokens_stmt, FetchAllTokensArgs, LocalDbToken,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};

pub async fn fetch_all_tokens<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchAllTokensArgs,
) -> Result<Vec<LocalDbToken>, LocalDbQueryError> {
    let stmt = build_fetch_all_tokens_stmt(&args)?;
    exec.query_json_retryable(&stmt).await
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
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_exactly() {
        let args = FetchAllTokensArgs {
            chain_ids: vec![1, 137],
            raindex_addresses: vec![Address::from([0x11; 20])],
        };

        let expected_stmt = build_fetch_all_tokens_stmt(&args).unwrap();

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_all_tokens(&exec, args).await;
        assert!(res.is_ok());

        let captured = store.borrow().clone();
        assert_eq!(captured.0, expected_stmt.sql);
    }
}
