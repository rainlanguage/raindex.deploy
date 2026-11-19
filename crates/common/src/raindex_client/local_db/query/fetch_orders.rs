use crate::local_db::query::fetch_orders::{
    build_fetch_orders_stmt, FetchOrdersActiveFilter, FetchOrdersArgs, FetchOrdersTokensFilter,
    LocalDbOrder,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::raindex_client::orders::GetOrdersFilters;

impl From<GetOrdersFilters> for FetchOrdersArgs {
    fn from(filters: GetOrdersFilters) -> Self {
        let filter = match filters.active {
            Some(true) => FetchOrdersActiveFilter::Active,
            Some(false) => FetchOrdersActiveFilter::Inactive,
            None => FetchOrdersActiveFilter::All,
        };
        let tokens = filters
            .tokens
            .map(|tokens| FetchOrdersTokensFilter {
                inputs: tokens.inputs.unwrap_or_default(),
                outputs: tokens.outputs.unwrap_or_default(),
            })
            .unwrap_or_default();

        FetchOrdersArgs {
            filter,
            owners: filters.owners,
            order_hash: filters.order_hash,
            tokens,
            raindex_addresses: filters.raindex_addresses.unwrap_or_default(),
            has_positive_output_vault_balance: filters.has_positive_output_vault_balance,
            ..FetchOrdersArgs::default()
        }
    }
}

pub async fn fetch_orders<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchOrdersArgs,
) -> Result<Vec<LocalDbOrder>, LocalDbQueryError> {
    let stmt = build_fetch_orders_stmt(&args)?;
    exec.query_json_retryable(&stmt).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raindex_client::orders::GetOrdersTokenFilter;
    use alloy::primitives::{address, b256, Address};
    use std::str::FromStr;

    #[test]
    fn from_get_orders_filters_builds_args() {
        let owner = Address::from_str("0x0123456789ABCDEF0123456789ABCDEF01234567").unwrap();
        let token = Address::from_str("0x89ABCDEF0123456789ABCDEF0123456789ABCDEF").unwrap();
        let raindex1 = address!("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let raindex2 = address!("0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB");
        let filters = GetOrdersFilters {
            owners: vec![owner],
            active: Some(true),
            order_hash: Some(b256!(
                "0x00000000000000000000000000000000000000000000000000000000deadbeef"
            )),
            tokens: Some(GetOrdersTokenFilter {
                inputs: Some(vec![token]),
                outputs: None,
            }),
            raindex_addresses: Some(vec![raindex1, raindex2]),
            has_positive_output_vault_balance: Some(true),
        };
        let args: FetchOrdersArgs = filters.into();
        assert!(matches!(args.filter, FetchOrdersActiveFilter::Active));
        assert_eq!(
            args.owners,
            vec![address!("0x0123456789abcdef0123456789abcdef01234567")]
        );
        assert_eq!(
            args.tokens.inputs,
            vec![address!("0x89abcdef0123456789abcdef0123456789abcdef")]
        );
        assert_eq!(args.tokens.outputs, Vec::<Address>::new());
        assert_eq!(
            args.order_hash,
            Some(b256!(
                "0x00000000000000000000000000000000000000000000000000000000deadbeef"
            ))
        );
        assert_eq!(args.raindex_addresses.len(), 2);
        assert_eq!(args.raindex_addresses[0], raindex1);
        assert_eq!(args.raindex_addresses[1], raindex2);
        assert_eq!(args.has_positive_output_vault_balance, Some(true));
    }

    #[test]
    fn from_get_orders_filters_none_raindex_addresses_becomes_empty_vec() {
        let filters = GetOrdersFilters {
            owners: vec![],
            active: None,
            order_hash: None,
            tokens: None,
            raindex_addresses: None,
            has_positive_output_vault_balance: None,
        };
        let args: FetchOrdersArgs = filters.into();
        assert!(args.raindex_addresses.is_empty());
    }

    #[test]
    fn from_get_orders_filters_single_raindex_address() {
        let raindex = address!("0xDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF");
        let filters = GetOrdersFilters {
            owners: vec![],
            active: None,
            order_hash: None,
            tokens: None,
            raindex_addresses: Some(vec![raindex]),
            has_positive_output_vault_balance: None,
        };
        let args: FetchOrdersArgs = filters.into();
        assert_eq!(args.raindex_addresses.len(), 1);
        assert_eq!(args.raindex_addresses[0], raindex);
    }

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use super::*;
        use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
        use crate::raindex_client::local_db::executor::JsCallbackExecutor;
        use alloy::primitives::{address, b256, Address};
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::prelude::Closure;
        use wasm_bindgen::JsCast;
        use wasm_bindgen_test::*;
        use wasm_bindgen_utils::prelude::*;

        fn raindex() -> Address {
            Address::from([0x11; 20])
        }

        #[wasm_bindgen_test]
        async fn wrapper_uses_builder_sql_exactly() {
            // Arrange args with mixed case and whitespace to exercise builder sanitization
            let mut args = FetchOrdersArgs::default();
            args.filter = FetchOrdersActiveFilter::Active;
            args.owners = vec![
                address!("0x0000000000000000000000000000000000000abc"),
                address!("0x00000000000000000000000000000000000000ef"),
            ];
            args.tokens.inputs = vec![address!("0x00000000000000000000000000000000000000aa")];
            args.order_hash = Some(b256!(
                "0x0000000000000000000000000000000000000000000000000000000000000001"
            ));
            args.chain_ids = vec![137];
            args.raindex_addresses = vec![raindex()];

            let expected_stmt = build_fetch_orders_stmt(&args).unwrap();

            let store = Rc::new(RefCell::new((
                String::new(),
                wasm_bindgen::JsValue::UNDEFINED,
            )));
            let callback = create_sql_capturing_callback("[]", store.clone());
            let exec = JsCallbackExecutor::from_ref(&callback);

            // Act
            let res = super::fetch_orders(&exec, args).await;

            // Assert
            assert!(res.is_ok());
            let (captured_sql, _captured_params) = store.borrow().clone();
            assert_eq!(captured_sql, expected_stmt.sql);
        }

        #[wasm_bindgen_test]
        async fn error_propagates_from_callback() {
            // Callback that returns WasmEncodedResult::Err
            let store = Rc::new(RefCell::new((
                String::new(),
                wasm_bindgen::JsValue::UNDEFINED,
            )));
            let store_clone = store.clone();
            let closure = Closure::wrap(Box::new(
                move |sql: String, params: wasm_bindgen::JsValue| -> wasm_bindgen::JsValue {
                    *store_clone.borrow_mut() = (sql, params);
                    let result: WasmEncodedResult<String> = WasmEncodedResult::Err {
                        value: None,
                        error: WasmEncodedError {
                            msg: "boom".to_string(),
                            readable_msg: "boom readable".to_string(),
                        },
                    };
                    serde_wasm_bindgen::to_value(&result).unwrap()
                },
            )
                as Box<dyn FnMut(String, wasm_bindgen::JsValue) -> wasm_bindgen::JsValue>);
            let callback: js_sys::Function = closure.as_ref().clone().unchecked_into();
            closure.forget();

            let args = FetchOrdersArgs {
                chain_ids: vec![1],
                ..FetchOrdersArgs::default()
            };
            let exec = JsCallbackExecutor::from_ref(&callback);
            let res = super::fetch_orders(&exec, args).await;
            assert!(res.is_err());
            let err = res.err().unwrap();
            let msg = err.to_string();
            assert!(msg.contains("boom readable"));
        }

        #[wasm_bindgen_test]
        async fn invalid_json_yields_deserialization_error() {
            // Build args and expected SQL
            let args = FetchOrdersArgs {
                chain_ids: vec![1],
                ..FetchOrdersArgs::default()
            };
            let expected_stmt = build_fetch_orders_stmt(&args).unwrap();

            // Callback returns Success with invalid JSON payload
            let store = Rc::new(RefCell::new((
                String::new(),
                wasm_bindgen::JsValue::UNDEFINED,
            )));
            let callback = create_sql_capturing_callback("not-json", store.clone());

            let exec = JsCallbackExecutor::from_ref(&callback);
            let res = super::fetch_orders(&exec, args).await;
            assert!(matches!(
                res,
                Err(LocalDbQueryError::Deserialization { .. })
            ));

            // Still should have executed with expected SQL
            assert_eq!(store.borrow().clone().0, expected_stmt.sql);
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

            let args = FetchOrdersArgs {
                chain_ids: vec![1],
                ..FetchOrdersArgs::default()
            };
            let exec = JsCallbackExecutor::from_ref(&callback);
            let res = super::fetch_orders(&exec, args).await;
            assert!(matches!(res, Err(LocalDbQueryError::InvalidResponse)));
        }
    }
}
