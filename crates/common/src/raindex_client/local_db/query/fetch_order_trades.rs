use crate::local_db::query::fetch_order_trades::{
    build_fetch_order_trades_batch_stmt, LocalDbOrderTrade, OrderHashFilter,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::local_db::RaindexIdentifier;
use alloy::primitives::B256;

/// Fetches the trades for a single order hash within the optional time window.
///
/// This is the single-order convenience wrapper over
/// [`fetch_order_trades_batch`]: it delegates to the batched fetcher with a
/// one-element hash slice, so the batched `WHERE order_hash IN (...)` query is
/// the single underlying path. Because the result is already filtered to the
/// one requested hash, the batch result is returned as-is.
pub async fn fetch_order_trades<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
    order_hash: B256,
    start_timestamp: Option<u64>,
    end_timestamp: Option<u64>,
) -> Result<Vec<LocalDbOrderTrade>, LocalDbQueryError> {
    fetch_order_trades_batch(
        exec,
        raindex_id,
        &[order_hash],
        start_timestamp,
        end_timestamp,
    )
    .await
}

/// Batched variant of [`fetch_order_trades`]: fetches trades for many order
/// hashes in a single `WHERE order_hash IN (...)` query, avoiding the N+1
/// pattern (and per-query connection overhead) of looping the single-hash
/// fetcher. Returns an empty vec for an empty input without touching the DB.
pub async fn fetch_order_trades_batch<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    raindex_id: &RaindexIdentifier,
    order_hashes: &[B256],
    start_timestamp: Option<u64>,
    end_timestamp: Option<u64>,
) -> Result<Vec<LocalDbOrderTrade>, LocalDbQueryError> {
    // Performance optimization, not a correctness guard: `These(&[])` already
    // builds a valid match-NONE query that returns zero rows, so this is the
    // same result the DB would give — we just skip the round-trip when we
    // already know the answer is empty.
    if order_hashes.is_empty() {
        return Ok(Vec::new());
    }
    let stmt = build_fetch_order_trades_batch_stmt(
        raindex_id,
        OrderHashFilter::These(order_hashes),
        start_timestamp,
        end_timestamp,
    )?;
    exec.query_json_retryable(&stmt).await
}

#[cfg(all(test, target_family = "wasm", feature = "browser-tests"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use alloy::primitives::{address, b256, hex, Address, U256};
    use js_sys::{Array, Function};
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::Closure;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::*;

    /// Builds a `LocalDbOrderTrade` for a given order hash so a test callback
    /// can return realistic, per-order rows (the only field that varies between
    /// orders for our discrimination is `order_hash` + `trade_id`).
    fn trade_row(order_hash: B256, trade_id: &str, block_timestamp: u64) -> LocalDbOrderTrade {
        LocalDbOrderTrade {
            chain_id: 111,
            trade_kind: "TakeOrder".to_string(),
            raindex: address!("0x7777777777777777777777777777777777777777"),
            order_hash,
            order_owner: address!("0x00000000000000000000000000000000000000aa"),
            order_nonce: "0x01".to_string(),
            transaction_hash: b256!(
                "0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            ),
            log_index: 0,
            block_number: 10,
            block_timestamp,
            transaction_sender: address!("0x00000000000000000000000000000000000000bb"),
            input_vault_id: U256::from(1u64),
            input_token: address!("0x00000000000000000000000000000000000000a1"),
            input_token_name: Some("Token A".to_string()),
            input_token_symbol: Some("TKA".to_string()),
            input_token_decimals: Some(18),
            input_delta: "0x01".to_string(),
            input_running_balance: Some("0x02".to_string()),
            output_vault_id: U256::from(2u64),
            output_token: address!("0x00000000000000000000000000000000000000b1"),
            output_token_name: Some("Token B".to_string()),
            output_token_symbol: Some("TKB".to_string()),
            output_token_decimals: Some(6),
            output_delta: "0x03".to_string(),
            output_running_balance: Some("0x04".to_string()),
            trade_id: trade_id.to_string(),
        }
    }

    /// A callback that mimics the SQLite `WHERE order_hash IN (...)` filter:
    /// it scans the *bound params* for each known order hash and returns only
    /// the seeded rows whose hash actually appears in the params. This makes the
    /// callback behave like the real DB — the wrapper only gets a given order's
    /// rows if it actually bound that order's hash into the batch query.
    fn order_hash_filtering_callback(seeded: Vec<LocalDbOrderTrade>) -> Function {
        let closure = Closure::wrap(Box::new(move |_sql: String, params: JsValue| -> JsValue {
            let mut bound: Vec<String> = Vec::new();
            if Array::is_array(&params) {
                let arr = Array::from(&params);
                for i in 0..arr.length() {
                    if let Some(s) = arr.get(i).as_string() {
                        bound.push(s);
                    }
                }
            }
            let matching: Vec<&LocalDbOrderTrade> = seeded
                .iter()
                .filter(|t| {
                    bound
                        .iter()
                        .any(|p| p == &hex::encode_prefixed(t.order_hash))
                })
                .collect();
            let json = serde_json::to_string(&matching).unwrap();
            let result = WasmEncodedResult::Success::<String> {
                value: json,
                error: None,
            };
            serde_wasm_bindgen::to_value(&result).unwrap()
        }) as Box<dyn FnMut(String, JsValue) -> JsValue>);
        let func: Function = closure.as_ref().clone().unchecked_into();
        closure.forget();
        func
    }

    #[wasm_bindgen_test]
    async fn wrapper_uses_batch_builder_sql_exactly() {
        // The single-order wrapper delegates to the batched fetcher with a
        // one-element hash slice, so the SQL it emits is exactly the batch
        // builder's `IN (...)` query (not the legacy `= ?3` form).
        let chain_id = 111;
        let raindex = Address::from([0x77; 20]);
        let order_hash =
            b256!("0x000000000000000000000000000000000000000000000000000000000000abcd");
        let start = Some(100);
        let end = Some(200);

        let expected_stmt = build_fetch_order_trades_batch_stmt(
            &RaindexIdentifier::new(chain_id, raindex),
            OrderHashFilter::These(&[order_hash]),
            start,
            end,
        )
        .unwrap();
        // The delegated SQL is the single-element IN form, not an equality.
        assert!(expected_stmt.sql.contains("tws.order_hash IN (?3)"));
        assert!(!expected_stmt.sql.contains("tws.order_hash = "));

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_order_trades(
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

    #[wasm_bindgen_test]
    async fn batch_wrapper_uses_builder_sql_exactly() {
        let chain_id = 111;
        let raindex = Address::from([0x77; 20]);
        let hash_a = b256!("0x000000000000000000000000000000000000000000000000000000000000abcd");
        let hash_b = b256!("0x000000000000000000000000000000000000000000000000000000000000ef01");
        let start = Some(100);
        let end = Some(200);

        let expected_stmt = build_fetch_order_trades_batch_stmt(
            &RaindexIdentifier::new(chain_id, raindex),
            OrderHashFilter::These(&[hash_a, hash_b]),
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

        let res = super::fetch_order_trades_batch(
            &exec,
            &RaindexIdentifier::new(chain_id, raindex),
            &[hash_a, hash_b],
            start,
            end,
        )
        .await;
        assert!(res.is_ok());

        let captured = store.borrow().clone();
        assert_eq!(captured.0, expected_stmt.sql);
    }

    #[wasm_bindgen_test]
    async fn batch_wrapper_short_circuits_empty_input() {
        let chain_id = 111;
        let raindex = Address::from([0x77; 20]);

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = super::fetch_order_trades_batch(
            &exec,
            &RaindexIdentifier::new(chain_id, raindex),
            &[],
            None,
            None,
        )
        .await
        .unwrap();

        // No DB call is made and an empty result is returned.
        assert!(res.is_empty());
        assert_eq!(store.borrow().0, String::new());
    }

    /// The single-order wrapper (used in production by `LocalDbOrders::trades_list`)
    /// now runs over the batch builder. This proves it returns exactly the
    /// requested order's trades — the same rows the batch path returns for a
    /// one-element list — and never the wrong order's rows, both orders' rows,
    /// or an empty result.
    #[wasm_bindgen_test]
    async fn single_order_wrapper_returns_only_that_orders_trades_via_batch() {
        let chain_id = 111;
        let raindex = address!("0x7777777777777777777777777777777777777777");
        let hash_a = b256!("0x000000000000000000000000000000000000000000000000000000000000aaaa");
        let hash_b = b256!("0x000000000000000000000000000000000000000000000000000000000000bbbb");

        // Two trades for order A, one for order B.
        let a1 = trade_row(hash_a, "trade-a1", 100);
        let a2 = trade_row(hash_a, "trade-a2", 150);
        let b1 = trade_row(hash_b, "trade-b1", 120);
        let seeded = vec![a1.clone(), a2.clone(), b1.clone()];

        let callback = order_hash_filtering_callback(seeded);
        let exec = JsCallbackExecutor::from_ref(&callback);
        let raindex_id = RaindexIdentifier::new(chain_id, raindex);

        // Single-order wrapper for A: must return exactly A's two trades.
        let via_wrapper = super::fetch_order_trades(&exec, &raindex_id, hash_a, None, None)
            .await
            .unwrap();
        // Distinguishes correct (2 rows for A) from empty / cross-order (B) /
        // both-orders (3 rows) results.
        assert_eq!(via_wrapper.len(), 2);
        assert!(via_wrapper.iter().all(|t| t.order_hash == hash_a));
        assert!(via_wrapper.iter().any(|t| t.trade_id == "trade-a1"));
        assert!(via_wrapper.iter().any(|t| t.trade_id == "trade-a2"));
        assert!(via_wrapper.iter().all(|t| t.order_hash != hash_b));
        assert!(via_wrapper.iter().all(|t| t.trade_id != "trade-b1"));

        // The batch path with the same one-element list returns the identical
        // rows: the single-order wrapper is now a faithful alias of the batch fn.
        let via_batch = super::fetch_order_trades_batch(&exec, &raindex_id, &[hash_a], None, None)
            .await
            .unwrap();
        assert_eq!(via_batch, via_wrapper);

        // And asking the wrapper for B returns only B's single trade, never A's
        // — proving the bound hash actually drives the result.
        let only_b = super::fetch_order_trades(&exec, &raindex_id, hash_b, None, None)
            .await
            .unwrap();
        assert_eq!(only_b.len(), 1);
        assert_eq!(only_b[0].order_hash, hash_b);
        assert_eq!(only_b[0].trade_id, "trade-b1");
    }
}
