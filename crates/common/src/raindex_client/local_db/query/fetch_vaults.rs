use crate::local_db::query::fetch_vaults::{
    build_fetch_vaults_count_stmt, build_fetch_vaults_stmt, extract_vaults_count, FetchVaultsArgs,
    LocalDbVault, LocalDbVaultsCountRow,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::raindex_client::vaults::GetVaultsFilters;

impl FetchVaultsArgs {
    pub fn from_filters(filters: GetVaultsFilters) -> Self {
        FetchVaultsArgs {
            chain_ids: Vec::new(),
            raindex_addresses: filters.raindex_addresses.unwrap_or_default(),
            owners: filters.owners,
            tokens: filters.tokens.unwrap_or_default(),
            hide_zero_balance: filters.hide_zero_balance,
            only_active_orders: filters.only_active_orders,
            page: None,
            page_size: None,
        }
    }
}

impl From<GetVaultsFilters> for FetchVaultsArgs {
    fn from(filters: GetVaultsFilters) -> Self {
        FetchVaultsArgs::from_filters(filters)
    }
}

pub async fn fetch_vaults<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchVaultsArgs,
) -> Result<Vec<LocalDbVault>, LocalDbQueryError> {
    let stmt = build_fetch_vaults_stmt(&args)?;
    exec.query_json_retryable(&stmt).await
}

pub async fn fetch_vaults_count<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchVaultsArgs,
) -> Result<u32, LocalDbQueryError> {
    let stmt = build_fetch_vaults_count_stmt(&args)?;
    let rows: Vec<LocalDbVaultsCountRow> = exec.query_json_retryable(&stmt).await?;
    Ok(extract_vaults_count(&rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn from_filters_builds_args() {
        let owner = address!("0x0123456789ABCDEF0123456789ABCDEF01234567");
        let token = address!("0x89ABCDEF0123456789ABCDEF0123456789ABCDEF");
        let raindex1 = address!("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let raindex2 = address!("0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB");
        let filters = GetVaultsFilters {
            owners: vec![owner],
            hide_zero_balance: true,
            tokens: Some(vec![token]),
            raindex_addresses: Some(vec![raindex1, raindex2]),
            only_active_orders: false,
        };
        let args = FetchVaultsArgs::from_filters(filters);
        assert_eq!(
            args.owners,
            vec![address!("0x0123456789abcdef0123456789abcdef01234567")]
        );
        assert_eq!(
            args.tokens,
            vec![address!("0x89abcdef0123456789abcdef0123456789abcdef")]
        );
        assert!(args.hide_zero_balance);
        assert_eq!(args.raindex_addresses.len(), 2);
        assert_eq!(args.raindex_addresses[0], raindex1);
        assert_eq!(args.raindex_addresses[1], raindex2);
        assert!(!args.only_active_orders);
    }

    #[test]
    fn from_filters_none_raindex_addresses_becomes_empty_vec() {
        let filters = GetVaultsFilters {
            owners: vec![],
            hide_zero_balance: false,
            tokens: None,
            raindex_addresses: None,
            only_active_orders: false,
        };
        let args = FetchVaultsArgs::from_filters(filters);
        assert!(args.raindex_addresses.is_empty());
    }

    #[test]
    fn from_filters_single_raindex_address() {
        let raindex = address!("0xDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF");
        let filters = GetVaultsFilters {
            owners: vec![],
            hide_zero_balance: false,
            tokens: None,
            raindex_addresses: Some(vec![raindex]),
            only_active_orders: false,
        };
        let args = FetchVaultsArgs::from_filters(filters);
        assert_eq!(args.raindex_addresses.len(), 1);
        assert_eq!(args.raindex_addresses[0], raindex);
    }

    #[test]
    fn from_filters_maps_only_active_orders() {
        let filters = GetVaultsFilters {
            owners: vec![],
            hide_zero_balance: false,
            tokens: None,
            raindex_addresses: None,
            only_active_orders: true,
        };
        let args = FetchVaultsArgs::from_filters(filters);
        assert!(args.only_active_orders);
    }

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use super::*;
        use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
        use crate::raindex_client::local_db::executor::JsCallbackExecutor;
        use alloy::primitives::{address, Address};
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen_test::*;
        use wasm_bindgen_utils::prelude::wasm_bindgen;

        #[wasm_bindgen_test]
        async fn wrapper_uses_builder_sql_exactly() {
            let mut args = FetchVaultsArgs::default();
            args.owners = vec![
                address!("0x0000000000000000000000000000000000000abc"),
                address!("0x00000000000000000000000000000000000000ef"),
            ];
            args.tokens = vec![address!("0x00000000000000000000000000000000000000aa")];
            args.hide_zero_balance = true;
            args.chain_ids = vec![1, 137];
            args.raindex_addresses = vec![
                Address::from([0x11; 20]),
                Address::from([0x22; 20]),
                Address::from([0x22; 20]),
            ];

            let expected_stmt = build_fetch_vaults_stmt(&args).unwrap();

            let store = Rc::new(RefCell::new((
                String::new(),
                wasm_bindgen::JsValue::UNDEFINED,
            )));
            let callback = create_sql_capturing_callback("[]", store.clone());
            let exec = JsCallbackExecutor::from_ref(&callback);

            let res = super::fetch_vaults(&exec, args).await;
            assert!(res.is_ok());

            let captured = store.borrow().clone();
            assert_eq!(captured.0, expected_stmt.sql);
        }
    }
}
