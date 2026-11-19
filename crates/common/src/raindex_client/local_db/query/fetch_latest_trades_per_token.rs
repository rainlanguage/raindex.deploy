use crate::local_db::query::fetch_latest_trades_per_token::{
    build_fetch_latest_trades_per_token_stmt, FetchLatestTradesPerTokenArgs, LatestTradeRow,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};
use crate::utils::timing::Timing;

pub async fn fetch_latest_trades_per_token<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchLatestTradesPerTokenArgs,
) -> Result<Vec<LatestTradeRow>, LocalDbQueryError> {
    if args.base_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let started = Timing::now();
    let base_tokens_count = args.base_tokens.len();
    let raindexes_count = args.raindex_addresses.len();
    let stmt = build_fetch_latest_trades_per_token_stmt(&args)?;
    let trades = exec
        .query_json_retryable::<Vec<LatestTradeRow>>(&stmt)
        .await?;
    tracing::info!(
        chain_id = args.chain_id,
        base_tokens_count,
        raindexes_count,
        rows = trades.len(),
        duration_ms = started.elapsed_ms(),
        "local DB latest market trades fetch completed"
    );
    Ok(trades)
}
