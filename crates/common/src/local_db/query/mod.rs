pub mod clear_raindex_data;
pub mod clear_tables;
pub mod create_tables;
pub mod create_views;
pub mod executor;
pub mod fetch_all_tokens;
pub mod fetch_db_metadata;
pub mod fetch_erc20_tokens_by_addresses;
pub mod fetch_last_synced_block;
pub mod fetch_latest_trades_per_token;
pub mod fetch_order_trades;
pub mod fetch_order_trades_count;
pub mod fetch_order_vaults_volume;
pub mod fetch_orders;
pub(crate) mod fetch_orders_common;
pub mod fetch_orders_count;
pub mod fetch_owner_trades;
pub(crate) mod fetch_owner_trades_common;
pub mod fetch_owner_trades_count;
pub mod fetch_store_addresses;
pub mod fetch_table_columns;
pub mod fetch_tables;
pub mod fetch_target_watermark;
pub mod fetch_trades;
pub mod fetch_trades_by_tx;
pub mod fetch_transaction_by_hash;
pub mod fetch_vault_balance_changes;
pub mod fetch_vaults;
pub mod insert_db_metadata;
pub mod integrity_check;
pub mod sql_script;
pub mod sql_statement;
pub mod sql_statement_batch;
pub mod update_last_synced_block;
pub mod upsert_derived_trades;
pub mod upsert_derived_vault_deltas;
pub mod upsert_target_watermark;
pub mod upsert_vault_balances;

pub use executor::LocalDbQueryExecutor;
pub use sql_script::SqlScript;
pub use sql_statement::{SqlBuildError, SqlStatement, SqlValue};
pub use sql_statement_batch::SqlStatementBatch;

use serde::de::DeserializeOwned;
use thiserror::Error;

/// Backend-neutral error representing failures when executing SQL against the
/// embedded local database. Specific backends should map their native error
/// types into this structure.
#[derive(Debug, Error, Clone)]
pub enum LocalDbQueryError {
    #[error("Database operation failed: {message}")]
    Database { message: String },

    #[error("Invalid response format from database")]
    InvalidResponse,

    #[error("Deserialization failed: {message}")]
    Deserialization { message: String },

    #[error("SQL build failed: {source}")]
    SqlBuild { source: SqlBuildError },

    #[error("Operation not implemented: {operation}")]
    NotImplemented { operation: String },

    #[error("not a raindex local-db: application_id {found:#010x} != expected {expected:#010x}")]
    NotARaindexDb { expected: i32, found: i32 },

    #[error("local-db schema version mismatch: user_version {found} != expected {expected}")]
    SchemaVersionMismatch { expected: i32, found: i32 },
}

impl LocalDbQueryError {
    pub fn database(message: impl Into<String>) -> Self {
        Self::Database {
            message: message.into(),
        }
    }

    pub fn invalid_response() -> Self {
        Self::InvalidResponse
    }

    pub fn deserialization(message: impl Into<String>) -> Self {
        Self::Deserialization {
            message: message.into(),
        }
    }

    pub fn not_implemented(operation: impl Into<String>) -> Self {
        Self::NotImplemented {
            operation: operation.into(),
        }
    }

    /// sqlite-web rejects ordinary requests while any tab holds a bulk SQL
    /// dump import open on the shared worker. The database is not unhealthy.
    pub fn is_import_in_progress(&self) -> bool {
        matches!(self, Self::Database { message } if message.contains("SQL dump import is in progress"))
    }

    /// The database worker was busy or not ready, so the request says nothing
    /// about the health of the stored data.
    pub fn is_worker_unavailable(&self) -> bool {
        self.is_import_in_progress()
            || matches!(self, Self::Database { message }
                if message.contains("Query timeout") || message.contains("Initialization pending"))
    }
}

impl From<SqlBuildError> for LocalDbQueryError {
    fn from(e: SqlBuildError) -> Self {
        LocalDbQueryError::SqlBuild { source: e }
    }
}

/// Helper trait for types that can be deserialized from DB JSON emitted by the
/// local database backend. Implementors must be deserializable from JSON.
pub trait FromDbJson: DeserializeOwned {}

impl<T> FromDbJson for T where T: DeserializeOwned {}
