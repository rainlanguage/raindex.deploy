use crate::local_db::query::fetch_order_trades::LocalDbOrderTrade;
use crate::local_db::query::fetch_order_trades_count::LocalDbTradeCountRow;
use crate::local_db::query::fetch_trades::{
    build_fetch_trades_count_stmt, build_fetch_trades_stmt, FetchTradesArgs,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::utils::timing::Timing;

pub async fn fetch_trades<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchTradesArgs,
) -> Result<Vec<LocalDbOrderTrade>, LocalDbQueryError> {
    let started = Timing::now();
    let chain_ids_count = args.chain_ids.len();
    let raindexes_count = args.raindex_addresses.len();
    let owners_count = args.owners.len();
    let takers_count = args.takers.len();
    let input_tokens_count = args.tokens.inputs.len();
    let output_tokens_count = args.tokens.outputs.len();
    let has_order_hash = args.order_hash.is_some();
    let has_time_filter = args.time_filter.start.is_some() || args.time_filter.end.is_some();
    let page = args.pagination.page;
    let page_size = args.pagination.page_size;
    let stmt = build_fetch_trades_stmt(&args)?;
    match exec
        .query_json_retryable::<Vec<LocalDbOrderTrade>>(&stmt)
        .await
    {
        Ok(trades) => {
            tracing::info!(
                chain_ids_count,
                raindexes_count,
                owners_count,
                takers_count,
                input_tokens_count,
                output_tokens_count,
                has_order_hash,
                has_time_filter,
                page,
                page_size,
                params_count = stmt.params().len(),
                rows = trades.len(),
                duration_ms = started.elapsed_ms(),
                "local DB getTrades fetch completed"
            );
            Ok(trades)
        }
        Err(err) => {
            tracing::error!(
                chain_ids_count,
                raindexes_count,
                owners_count,
                takers_count,
                input_tokens_count,
                output_tokens_count,
                has_order_hash,
                has_time_filter,
                page,
                page_size,
                params_count = stmt.params().len(),
                duration_ms = started.elapsed_ms(),
                error = %err,
                "local DB getTrades fetch failed"
            );
            Err(err)
        }
    }
}

pub async fn fetch_trades_count<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchTradesArgs,
) -> Result<Vec<LocalDbTradeCountRow>, LocalDbQueryError> {
    let started = Timing::now();
    let chain_ids_count = args.chain_ids.len();
    let raindexes_count = args.raindex_addresses.len();
    let owners_count = args.owners.len();
    let takers_count = args.takers.len();
    let input_tokens_count = args.tokens.inputs.len();
    let output_tokens_count = args.tokens.outputs.len();
    let has_order_hash = args.order_hash.is_some();
    let has_time_filter = args.time_filter.start.is_some() || args.time_filter.end.is_some();
    let stmt = build_fetch_trades_count_stmt(&args)?;
    match exec
        .query_json_retryable::<Vec<LocalDbTradeCountRow>>(&stmt)
        .await
    {
        Ok(rows) => {
            tracing::info!(
                chain_ids_count,
                raindexes_count,
                owners_count,
                takers_count,
                input_tokens_count,
                output_tokens_count,
                has_order_hash,
                has_time_filter,
                params_count = stmt.params().len(),
                rows = rows.len(),
                trade_count = rows.first().map(|row| row.trade_count),
                duration_ms = started.elapsed_ms(),
                "local DB getTrades count completed"
            );
            Ok(rows)
        }
        Err(err) => {
            tracing::error!(
                chain_ids_count,
                raindexes_count,
                owners_count,
                takers_count,
                input_tokens_count,
                output_tokens_count,
                has_order_hash,
                has_time_filter,
                params_count = stmt.params().len(),
                duration_ms = started.elapsed_ms(),
                error = %err,
                "local DB getTrades count failed"
            );
            Err(err)
        }
    }
}

#[cfg(all(test, target_family = "wasm"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::local_db::executor::tests::create_sql_capturing_callback;
    use crate::raindex_client::local_db::executor::JsCallbackExecutor;
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::prelude::*;

    #[wasm_bindgen_test]
    async fn wrapper_uses_builder_sql_exactly() {
        let args = FetchTradesArgs {
            chain_ids: vec![137, 42161],
            raindex_addresses: vec![],
            ..Default::default()
        };

        let expected_stmt = build_fetch_trades_stmt(&args).unwrap();

        let store = Rc::new(RefCell::new((
            String::new(),
            wasm_bindgen::JsValue::UNDEFINED,
        )));
        let callback = create_sql_capturing_callback("[]", store.clone());
        let exec = JsCallbackExecutor::from_ref(&callback);

        let res = fetch_trades(&exec, args).await;
        assert!(res.is_ok());

        let captured = store.borrow().clone();
        assert_eq!(captured.0, expected_stmt.sql);
    }
}
