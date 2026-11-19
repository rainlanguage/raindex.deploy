use async_trait::async_trait;
#[cfg(target_family = "wasm")]
use std::sync::Arc;

use super::{FromDbJson, LocalDbQueryError, SqlStatement, SqlStatementBatch};

/// Backend-neutral executor for running SQL against the local DB backend.
///
/// Implementations provide text and JSON query methods that map backend
/// errors into `LocalDbQueryError` and deserialize JSON into target types
/// via the `FromDbJson` bound.
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
pub trait LocalDbQueryExecutor {
    async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError>;

    /// Execute a data-only dump. Backends can override this to avoid building
    /// one owned statement for every line of the dump.
    #[cfg(target_family = "wasm")]
    async fn execute_sql_dump(&self, dump: Arc<String>) -> Result<(), LocalDbQueryError> {
        let statements: Vec<_> = dump.lines().map(SqlStatement::new).collect();
        self.execute_batch(&SqlStatementBatch::from(statements))
            .await
    }

    /// Atomically import a browser bootstrap cohort with a single index rebuild.
    /// Implementations must not commit individual dumps separately.
    #[cfg(target_family = "wasm")]
    async fn execute_sql_dumps(&self, _dumps: Vec<Arc<String>>) -> Result<(), LocalDbQueryError> {
        Err(LocalDbQueryError::not_implemented("execute_sql_dumps"))
    }

    async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
    where
        T: FromDbJson;

    /// Query a statement explicitly known to be safe to repeat. Browser
    /// backends may retry temporary worker errors and timeouts. The caller
    /// must ensure the SQL is read-only or idempotent; writes with an unknown
    /// commit outcome must use `query_json` instead.
    #[cfg(target_family = "wasm")]
    async fn query_json_retryable<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
    where
        T: FromDbJson,
    {
        self.query_json(stmt).await
    }

    /// Native queries keep their original execution behavior. Returning the
    /// existing future directly avoids imposing an additional `Self: Sync`
    /// bound through an async-trait default implementation.
    #[cfg(not(target_family = "wasm"))]
    fn query_json_retryable<'a, T>(
        &'a self,
        stmt: &'a SqlStatement,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T, LocalDbQueryError>> + Send + 'a>,
    >
    where
        T: FromDbJson + 'a,
    {
        self.query_json(stmt)
    }

    async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError>;

    async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError>;
}
