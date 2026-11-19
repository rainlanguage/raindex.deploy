use crate::local_db::query::fetch_erc20_tokens_by_addresses::{build_fetch_stmt, Erc20TokenRow};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::local_db::RaindexIdentifier;
use alloy::primitives::Address;

pub async fn fetch_erc20_tokens_by_addresses<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
    addresses: &[Address],
) -> Result<Vec<Erc20TokenRow>, LocalDbQueryError> {
    if let Some(stmt) = build_fetch_stmt(raindex_id, addresses)? {
        exec.query_json_retryable(&stmt).await
    } else {
        Ok(vec![])
    }
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::str::FromStr;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn empty_addresses_short_circuits_and_executes_no_sql() {
        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);
        let res = super::fetch_erc20_tokens_by_addresses(
            &exec,
            &RaindexIdentifier::new(1, Address::ZERO),
            &[],
        )
        .await;
        assert!(res.is_ok());
        assert!(res.unwrap().is_empty());
        assert!(store.borrow().0.is_empty());
    }

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_exactly() {
        let addrs = vec![
            Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            Address::from_str("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        ];
        let expected_stmt = build_fetch_stmt(&RaindexIdentifier::new(10, Address::ZERO), &addrs)
            .unwrap()
            .unwrap();

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_erc20_tokens_by_addresses(
            &exec,
            &RaindexIdentifier::new(10, Address::ZERO),
            &addrs,
        )
        .await;
        assert!(res.is_ok());

        let (captured_sql, captured_params) = store.borrow().clone();
        assert_eq!(captured_sql, expected_stmt.sql);

        // Also assert parameters are encoded and passed as expected
        // The executor passes `undefined` when empty, otherwise an array with Text / BigInt entries
        if expected_stmt.params().is_empty() {
            assert!(captured_params.is_undefined());
        } else {
            use wasm_bindgen::JsCast;

            assert!(
                js_sys::Array::is_array(&captured_params),
                "expected params array"
            );
            let captured_array = js_sys::Array::from(&captured_params);
            assert_eq!(captured_array.length(), expected_stmt.params().len() as u32);

            for (idx, expected_param) in expected_stmt.params().iter().enumerate() {
                let value = captured_array.get(idx as u32);
                match expected_param {
                    crate::local_db::query::SqlValue::Text(expected) => {
                        assert_eq!(
                            value.as_string().unwrap(),
                            *expected,
                            "text param mismatch at index {idx}"
                        );
                    }
                    crate::local_db::query::SqlValue::U64(expected) => {
                        let bigint = value
                            .dyn_into::<js_sys::BigInt>()
                            .expect("numeric params should be BigInt");
                        let numeric = bigint.to_string(10).unwrap().as_string().unwrap();
                        assert_eq!(
                            numeric,
                            expected.to_string(),
                            "numeric param mismatch at index {idx}"
                        );
                    }
                    crate::local_db::query::SqlValue::I64(expected) => {
                        let bigint = value
                            .dyn_into::<js_sys::BigInt>()
                            .expect("numeric params should be BigInt");
                        let numeric = bigint.to_string(10).unwrap().as_string().unwrap();
                        assert_eq!(
                            numeric,
                            expected.to_string(),
                            "numeric param mismatch at index {idx}"
                        );
                    }
                    crate::local_db::query::SqlValue::Null => {
                        assert!(value.is_null(), "null param mismatch at index {idx}");
                    }
                }
            }
        }
    }
}
