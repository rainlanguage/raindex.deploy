use crate::local_db::query::fetch_order_trades::LocalDbOrderTrade;
use crate::local_db::query::fetch_owner_trades::{
    build_fetch_owner_trades_stmt, FetchOwnerTradesArgs,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};

pub async fn fetch_owner_trades<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchOwnerTradesArgs,
) -> Result<Vec<LocalDbOrderTrade>, LocalDbQueryError> {
    let stmt = build_fetch_owner_trades_stmt(&args)?;
    exec.query_json_retryable(&stmt).await
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use alloy::primitives::address;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_exactly() {
        let owner = address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let args = FetchOwnerTradesArgs {
            owner,
            chain_ids: vec![137, 42161],
            raindex_addresses: vec![],
            time_filter: Default::default(),
            pagination: Default::default(),
        };

        let expected_stmt = build_fetch_owner_trades_stmt(&args).unwrap();

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = fetch_owner_trades(&exec, args).await;
        assert!(res.is_ok());

        let captured = store.borrow().clone();
        assert_eq!(captured.0, expected_stmt.sql);
    }
}
