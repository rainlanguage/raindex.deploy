use crate::local_db::query::fetch_order_vaults_volume::{
    build_fetch_order_vaults_volume_stmt, LocalDbVaultVolume,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::local_db::RaindexIdentifier;
use alloy::primitives::B256;

pub async fn fetch_order_vaults_volume<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
    order_hash: B256,
    start_timestamp: Option<u64>,
    end_timestamp: Option<u64>,
) -> Result<Vec<LocalDbVaultVolume>, LocalDbQueryError> {
    let stmt = build_fetch_order_vaults_volume_stmt(
        raindex_id,
        order_hash,
        start_timestamp,
        end_timestamp,
    )?;
    exec.query_json_retryable(&stmt).await
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
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_exactly() {
        let chain_id = 111;
        let raindex = Address::from([0x77; 20]);
        let order_hash =
            b256!("0x000000000000000000000000000000000000000000000000000000000000abcd");
        let start = Some(100);
        let end = Some(200);

        let expected_stmt = build_fetch_order_vaults_volume_stmt(
            &RaindexIdentifier::new(chain_id, raindex),
            order_hash,
            start,
            end,
        )
        .unwrap();

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_order_vaults_volume(
            &exec,
            &RaindexIdentifier::new(chain_id, raindex),
            order_hash,
            start,
            end,
        )
        .await;
        assert!(res.is_ok());

        let captured = store.borrow().clone();
        assert_eq!(captured.0, expected_stmt.sql);
    }
}
