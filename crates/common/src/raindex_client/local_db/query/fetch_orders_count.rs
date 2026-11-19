use crate::local_db::query::fetch_orders::FetchOrdersArgs;
use crate::local_db::query::fetch_orders_count::{
    build_fetch_orders_count_stmt, extract_orders_count, LocalDbOrdersCountRow,
};
use crate::local_db::query::{LocalDbQueryError, LocalDbQueryExecutor};

pub async fn fetch_orders_count<E: LocalDbQueryExecutor + ?Sized>(
    exec: &E,
    args: FetchOrdersArgs,
) -> Result<u32, LocalDbQueryError> {
    let stmt = build_fetch_orders_count_stmt(&args)?;
    let rows: Vec<LocalDbOrdersCountRow> = exec.query_json_retryable(&stmt).await?;
    Ok(extract_orders_count(&rows))
}
