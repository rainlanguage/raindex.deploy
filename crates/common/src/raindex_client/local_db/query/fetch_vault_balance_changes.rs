use crate::local_db::query::fetch_vault_balance_changes::{
    build_fetch_balance_changes_stmt, LocalDbVaultBalanceChange,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::local_db::RaindexIdentifier;
use crate::raindex_client::vaults::VaultBalanceChangeFilter;
use crate::types::VaultBalanceChangeKind;
use alloy::primitives::{Address, U256};

pub async fn fetch_vault_balance_changes<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
    vault_id: U256,
    token: Address,
    owner: Address,
    filter_types: Option<&[VaultBalanceChangeFilter]>,
) -> Result<Vec<LocalDbVaultBalanceChange>, LocalDbQueryError> {
    let filter_kinds: Option<Vec<VaultBalanceChangeKind>> =
        filter_types.map(|filters| filters.iter().map(|f| f.to_kind()).collect());
    let stmt = build_fetch_balance_changes_stmt(
        raindex_id,
        vault_id,
        token,
        owner,
        filter_kinds.as_deref(),
    )?;
    exec.query_json_retryable(&stmt).await
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use alloy::primitives::{address, Address, U256};
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_exactly() {
        let vault_id = U256::from(1);
        let token = address!("0x00000000000000000000000000000000000000aa");
        let raindex = Address::from([0x51; 20]);
        let owner = address!("0x00000000000000000000000000000000000000f1");
        let expected_stmt = build_fetch_balance_changes_stmt(
            &RaindexIdentifier::new(1, raindex),
            vault_id,
            token,
            owner,
            None,
        )
        .unwrap();

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_vault_balance_changes(
            &exec,
            &RaindexIdentifier::new(1, raindex),
            vault_id,
            token,
            owner,
            None,
        )
        .await;
        assert!(res.is_ok());
        assert_eq!(store.borrow().clone().0, expected_stmt.sql);
    }

    #[wasm_bindgen_test]
    async fn wrapper_returns_rows_when_present() {
        let vault_id = U256::from(1);
        let token = address!("0x00000000000000000000000000000000000000bb");
        let raindex = Address::from([0x61; 20]);
        let owner = address!("0x0000000000000000000000000000000000000011");
        let expected_stmt = build_fetch_balance_changes_stmt(
            &RaindexIdentifier::new(1, raindex),
            vault_id,
            token,
            owner,
            None,
        )
        .unwrap();

        let row_json = r#"[{
            "transactionHash":"0x0000000000000000000000000000000000000000000000000000000000000abc",
            "logIndex":1,
            "blockNumber":100,
            "blockTimestamp":999,
            "owner":"0x0000000000000000000000000000000000000011",
            "transactionSender":"0x0000000000000000000000000000000000000011",
            "changeType":"deposit",
            "token":"0x00000000000000000000000000000000000000bb",
            "vaultId":"0x01",
            "delta":"0x01",
            "runningBalance":"0x01"
        }]"#;

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback(row_json, store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_vault_balance_changes(
            &exec,
            &RaindexIdentifier::new(1, raindex),
            vault_id,
            token,
            owner,
            None,
        )
        .await;
        assert!(res.is_ok());
        let rows = res.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(store.borrow().clone().0, expected_stmt.sql);
    }

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_with_filter_types() {
        let vault_id = U256::from(1);
        let token = address!("0x00000000000000000000000000000000000000aa");
        let raindex = Address::from([0x51; 20]);
        let owner = address!("0x00000000000000000000000000000000000000f1");
        let filter_types = vec![
            VaultBalanceChangeFilter::Deposit,
            VaultBalanceChangeFilter::Withdrawal,
        ];
        let filter_kinds: Vec<_> = filter_types.iter().map(|f| f.to_kind()).collect();
        let expected_stmt = build_fetch_balance_changes_stmt(
            &RaindexIdentifier::new(1, raindex),
            vault_id,
            token,
            owner,
            Some(&filter_kinds),
        )
        .unwrap();

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_vault_balance_changes(
            &exec,
            &RaindexIdentifier::new(1, raindex),
            vault_id,
            token,
            owner,
            Some(&filter_types),
        )
        .await;
        assert!(res.is_ok());
        assert_eq!(store.borrow().clone().0, expected_stmt.sql);
        assert!(expected_stmt.sql.contains("change_type IN"));
    }
}
