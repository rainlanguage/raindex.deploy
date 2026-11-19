use crate::local_db::query::clear_raindex_data::clear_raindex_data_batch;
use crate::local_db::query::clear_tables::{clear_tables_batch, vacuum_stmt};
use crate::local_db::query::create_tables::create_tables_batch;
use crate::local_db::query::create_tables::REQUIRED_TABLES;
use crate::local_db::query::create_views::create_views_batch;
use crate::local_db::query::fetch_db_metadata::{fetch_db_metadata_stmt, DbMetadataRow};
use crate::local_db::query::fetch_tables::{fetch_tables_stmt, TableResponse};
use crate::local_db::query::fetch_target_watermark::fetch_target_watermark_stmt;
use crate::local_db::query::fetch_target_watermark::TargetWatermarkRow;
use crate::local_db::query::insert_db_metadata::insert_db_metadata_stmt;
use crate::local_db::query::integrity_check::{integrity_check_stmt, IntegrityCheckRow};
use crate::local_db::query::{LocalDbQueryExecutor, SqlStatementBatch};
use crate::local_db::LocalDbError;
use crate::local_db::RaindexIdentifier;
use async_trait::async_trait;
use raindex_app_settings::local_db_manifest::DB_SCHEMA_VERSION;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub raindex_id: RaindexIdentifier,
    pub dump_sql: Option<Arc<String>>,
    pub dump_stmt: Option<SqlStatementBatch>,
    pub latest_block: u64,
    pub block_number_threshold: u32,
    pub deployment_block: u64,
}

/// Bootstrap state snapshot used by environment orchestration to decide actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapState {
    pub has_required_tables: bool,
    pub last_synced_block: Option<u64>,
}

/// Ensures the database is ready for incremental sync and applies optional
/// data‑only seed dumps per environment policy.
///
/// Responsibilities (concrete):
/// - Ensure schema tables exist. Dumps must not include DDL.
/// - Version gate via `db_metadata` (read/init, fail/reset on mismatch per
///   environment policy).
///
/// Implementors should orchestrate bootstrap via `run` and may use shared
/// helpers for the lower-level operations exposed as trait methods here.
#[async_trait(?Send)]
pub trait BootstrapPipeline {
    async fn ensure_schema<DB>(
        &self,
        db: &DB,
        db_schema_version: Option<u32>,
    ) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let rows = db
            .query_json_retryable::<Vec<DbMetadataRow>>(&fetch_db_metadata_stmt())
            .await?;
        if let Some(row) = rows.first() {
            let expected = db_schema_version.unwrap_or(DB_SCHEMA_VERSION);
            if row.db_schema_version != expected {
                return Err(LocalDbError::SchemaVersionMismatch {
                    expected,
                    found: row.db_schema_version,
                });
            }
            return Ok(());
        }
        return Err(LocalDbError::MissingDbMetadataRow);
    }

    async fn inspect_state<DB>(
        &self,
        db: &DB,
        raindex_id: &RaindexIdentifier,
    ) -> Result<BootstrapState, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let existing: Vec<TableResponse> = db.query_json_retryable(&fetch_tables_stmt()).await?;
        let existing_set: HashSet<String> = existing
            .into_iter()
            .map(|t| t.name.to_ascii_lowercase())
            .collect();

        let has_required_tables = REQUIRED_TABLES
            .iter()
            .all(|&t| existing_set.contains(&t.to_ascii_lowercase()));

        let last_synced_block = if existing_set.contains("target_watermarks") {
            let rows: Vec<TargetWatermarkRow> = db
                .query_json_retryable(&fetch_target_watermark_stmt(raindex_id))
                .await?;
            rows.first().map(|r| r.last_block)
        } else {
            None
        };

        Ok(BootstrapState {
            has_required_tables,
            last_synced_block,
        })
    }

    async fn reset_db<DB>(
        &self,
        db: &DB,
        db_schema_version: Option<u32>,
    ) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        db.execute_batch(&clear_tables_batch()).await?;
        db.query_text(&vacuum_stmt()).await?;
        db.execute_batch(&create_tables_batch()).await?;
        db.query_text(&insert_db_metadata_stmt(
            db_schema_version.unwrap_or(DB_SCHEMA_VERSION),
        ))
        .await?;
        db.execute_batch(&create_views_batch()).await?;
        Ok(())
    }

    async fn check_integrity<DB>(&self, db: &DB) -> Result<bool, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let rows: Vec<IntegrityCheckRow> = db.query_json_retryable(&integrity_check_stmt()).await?;
        let is_healthy = rows
            .first()
            .map(|row| row.quick_check.eq_ignore_ascii_case("ok"))
            .unwrap_or(false);
        Ok(is_healthy)
    }

    async fn clear_raindex_data<DB>(
        &self,
        db: &DB,
        raindex_id: &RaindexIdentifier,
    ) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let batch = clear_raindex_data_batch(raindex_id);
        db.execute_batch(&batch).await?;
        Ok(())
    }

    async fn engine_run<DB>(&self, _: &DB, _: &BootstrapConfig) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        Err(LocalDbError::InvalidBootstrapImplementation)
    }

    async fn runner_run<DB>(&self, _: &DB, _: Option<u32>) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        Err(LocalDbError::InvalidBootstrapImplementation)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Mutex;

    use super::*;
    use crate::local_db::query::clear_tables::{clear_tables_batch, vacuum_stmt};
    use crate::local_db::query::create_tables::create_tables_batch;
    use crate::local_db::query::create_views::create_views_batch;
    use crate::local_db::query::fetch_db_metadata::{fetch_db_metadata_stmt, DbMetadataRow};
    use crate::local_db::query::fetch_tables::{fetch_tables_stmt, TableResponse};
    use crate::local_db::query::{FromDbJson, LocalDbQueryError, SqlStatement, SqlStatementBatch};
    use alloy::primitives::{Address, Bytes};
    use async_trait::async_trait;
    use serde_json::json;

    #[derive(Default)]
    struct MockDb {
        json_map: HashMap<String, String>,
        text_map: HashMap<String, String>,
        calls_text: Mutex<Vec<String>>,
    }

    impl MockDb {
        fn with_json(mut self, stmt: &SqlStatement, value: serde_json::Value) -> Self {
            self.json_map
                .insert(stmt.sql().to_string(), value.to_string());
            self
        }
        fn with_text(mut self, stmt: &SqlStatement, value: &str) -> Self {
            self.text_map
                .insert(stmt.sql().to_string(), value.to_string());
            self
        }
        fn with_views(self) -> Self {
            create_views_batch()
                .statements()
                .iter()
                .fold(self, |db, stmt| db.with_text(stmt, "ok"))
        }
        fn with_batch(self, batch: &SqlStatementBatch) -> Self {
            batch
                .statements()
                .iter()
                .fold(self, |db, stmt| db.with_text(stmt, "ok"))
        }
        fn calls(&self) -> Vec<String> {
            self.calls_text.lock().unwrap().clone()
        }
    }

    fn reset_prefix_sql() -> Vec<String> {
        let mut statements = clear_tables_batch().statements().to_vec();
        statements.push(vacuum_stmt());
        statements.extend(create_tables_batch().statements().iter().cloned());
        statements
            .iter()
            .map(|stmt| stmt.sql().to_string())
            .collect()
    }

    struct RecordingTextExecutor {
        result: Mutex<Option<Result<(), LocalDbError>>>,
        captured_sql: Mutex<Vec<String>>,
    }

    impl RecordingTextExecutor {
        fn new(result: Result<(), LocalDbError>) -> Self {
            Self {
                result: Mutex::new(Some(result)),
                captured_sql: Mutex::new(Vec::new()),
            }
        }

        fn succeed() -> Self {
            Self::new(Ok(()))
        }

        fn fail(err: LocalDbError) -> Self {
            Self::new(Err(err))
        }

        fn captured_sql(&self) -> Vec<String> {
            self.captured_sql.lock().unwrap().clone()
        }
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for RecordingTextExecutor {
        async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            let mut captured = self.captured_sql.lock().unwrap();
            captured.extend(batch.statements().iter().map(|stmt| stmt.sql().to_string()));

            let outcome = self
                .result
                .lock()
                .unwrap()
                .take()
                .expect("execute_batch called more than expected");

            match outcome {
                Ok(()) => Ok(()),
                Err(LocalDbError::LocalDbQueryError(inner)) => Err(inner),
                Err(err) => Err(LocalDbQueryError::database(err.to_string())),
            }
        }

        async fn query_json<T>(&self, _: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            panic!("query_json should not be called in these tests");
        }

        async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            self.captured_sql
                .lock()
                .unwrap()
                .push(stmt.sql().to_string());

            let outcome = self
                .result
                .lock()
                .unwrap()
                .take()
                .expect("query_text called more than expected");

            match outcome {
                Ok(()) => Ok(String::from("ok")),
                Err(LocalDbError::LocalDbQueryError(inner)) => Err(inner),
                Err(err) => Err(LocalDbQueryError::database(err.to_string())),
            }
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    #[derive(Default, Clone, Copy)]
    struct TestBootstrapPipeline;

    impl TestBootstrapPipeline {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait(?Send)]
    impl BootstrapPipeline for TestBootstrapPipeline {}

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for MockDb {
        async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            for stmt in batch {
                let _ = self.query_text(stmt).await?;
            }
            Ok(())
        }

        async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            let sql = stmt.sql();
            let Some(body) = self.json_map.get(sql) else {
                return Err(LocalDbQueryError::database("no json for sql"));
            };
            serde_json::from_str::<T>(body)
                .map_err(|e| LocalDbQueryError::deserialization(e.to_string()))
        }

        async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            let sql = stmt.sql();
            self.calls_text.lock().unwrap().push(sql.to_string());
            let Some(body) = self.text_map.get(sql) else {
                return Err(LocalDbQueryError::database("no text for sql"));
            };
            Ok(body.clone())
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    #[tokio::test]
    async fn ensure_schema_ok_with_matching_version() {
        let adapter = TestBootstrapPipeline::new();
        let db_row = DbMetadataRow {
            id: 1,
            db_schema_version: DB_SCHEMA_VERSION,
            created_at: None,
            updated_at: None,
        };
        let db = MockDb::default().with_json(&fetch_db_metadata_stmt(), json!([db_row]));

        adapter.ensure_schema(&db, None).await.expect("schema ok");
    }

    #[tokio::test]
    async fn ensure_schema_err_on_mismatch() {
        let adapter = TestBootstrapPipeline::new();
        let db_row = DbMetadataRow {
            id: 1,
            db_schema_version: DB_SCHEMA_VERSION + 1,
            created_at: None,
            updated_at: None,
        };
        let db = MockDb::default().with_json(&fetch_db_metadata_stmt(), json!([db_row]));

        let err = adapter.ensure_schema(&db, None).await.unwrap_err();
        match err {
            LocalDbError::SchemaVersionMismatch { expected, found } => {
                assert_eq!(expected, DB_SCHEMA_VERSION);
                assert_eq!(found, DB_SCHEMA_VERSION + 1);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn ensure_schema_honors_override_ok() {
        let adapter = TestBootstrapPipeline::new();
        let override_version = DB_SCHEMA_VERSION + 7;
        let db_row = DbMetadataRow {
            id: 1,
            db_schema_version: override_version,
            created_at: None,
            updated_at: None,
        };
        let db = MockDb::default().with_json(&fetch_db_metadata_stmt(), json!([db_row]));

        adapter
            .ensure_schema(&db, Some(override_version))
            .await
            .expect("schema ok with override");
    }

    #[tokio::test]
    async fn ensure_schema_honors_override_mismatch() {
        let adapter = TestBootstrapPipeline::new();
        let row_version = DB_SCHEMA_VERSION + 3;
        let db_row = DbMetadataRow {
            id: 1,
            db_schema_version: row_version,
            created_at: None,
            updated_at: None,
        };
        let db = MockDb::default().with_json(&fetch_db_metadata_stmt(), json!([db_row]));

        let err = adapter
            .ensure_schema(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap_err();
        match err {
            LocalDbError::SchemaVersionMismatch { expected, found } => {
                assert_eq!(expected, DB_SCHEMA_VERSION);
                assert_eq!(found, row_version);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn ensure_schema_err_on_missing_row() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default().with_json(&fetch_db_metadata_stmt(), json!([]));
        let err = adapter.ensure_schema(&db, None).await.unwrap_err();
        match err {
            LocalDbError::MissingDbMetadataRow => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn inspect_state_tables_and_last_synced_block() {
        let adapter = TestBootstrapPipeline::new();
        // Provide all required tables
        let tables_json = serde_json::to_value(
            REQUIRED_TABLES
                .iter()
                .map(|&name| TableResponse {
                    name: name.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();

        // Watermark row present
        let raindex_id = RaindexIdentifier::new(1, Address::ZERO);
        let watermark_stmt = fetch_target_watermark_stmt(&raindex_id);
        let watermark_json = json!([TargetWatermarkRow {
            chain_id: raindex_id.chain_id,
            raindex_address: raindex_id.raindex_address,
            last_block: 123,
            last_hash: Bytes::from_str("0xbeef").unwrap(),
            updated_at: 1,
        }]);

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(&watermark_stmt, watermark_json);

        let state = adapter.inspect_state(&db, &raindex_id).await.unwrap();
        assert!(state.has_required_tables);
        assert_eq!(state.last_synced_block, Some(123));
    }

    #[tokio::test]
    async fn inspect_state_missing_tables_means_not_ready_and_no_watermark_query() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default().with_json(&fetch_tables_stmt(), json!([]));
        let raindex_id = RaindexIdentifier::new(1, Address::ZERO);
        let state = adapter.inspect_state(&db, &raindex_id).await.unwrap();
        assert!(!state.has_required_tables);
        assert_eq!(state.last_synced_block, None);
    }

    #[tokio::test]
    async fn inspect_state_missing_only_watermark_table() {
        let adapter = TestBootstrapPipeline::new();
        // All required tables except `target_watermarks`.
        let names: Vec<&str> = REQUIRED_TABLES
            .iter()
            .copied()
            .filter(|t| !t.eq_ignore_ascii_case("target_watermarks"))
            .collect();
        assert!(names.len() + 1 == REQUIRED_TABLES.len());
        let tables_json = serde_json::to_value(
            names
                .into_iter()
                .map(|name| TableResponse {
                    name: name.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();

        let db = MockDb::default().with_json(&fetch_tables_stmt(), tables_json);
        let raindex_id = RaindexIdentifier {
            chain_id: 1,
            raindex_address: Address::ZERO,
        };
        let state = adapter.inspect_state(&db, &raindex_id).await.unwrap();
        assert!(!state.has_required_tables);
        assert_eq!(state.last_synced_block, None);
    }

    #[tokio::test]
    async fn inspect_state_watermark_table_present_but_empty() {
        let adapter = TestBootstrapPipeline::new();
        let tables_json = serde_json::to_value(
            REQUIRED_TABLES
                .iter()
                .map(|&name| TableResponse {
                    name: name.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();

        let raindex_id = RaindexIdentifier::new(1, Address::ZERO);
        let watermark_stmt = fetch_target_watermark_stmt(&raindex_id);

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(&watermark_stmt, json!([]));

        let state = adapter.inspect_state(&db, &raindex_id).await.unwrap();
        assert!(state.has_required_tables);
        assert_eq!(state.last_synced_block, None);
    }

    #[tokio::test]
    async fn inspect_state_table_names_case_insensitive() {
        let adapter = TestBootstrapPipeline::new();
        let tables_json = serde_json::to_value(
            REQUIRED_TABLES
                .iter()
                .enumerate()
                .map(|(i, &name)| {
                    let cased = if i % 2 == 0 {
                        name.to_ascii_uppercase()
                    } else {
                        name.to_ascii_lowercase()
                    };
                    TableResponse { name: cased }
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();

        let raindex_id = RaindexIdentifier::new(1, Address::ZERO);
        let watermark_stmt = fetch_target_watermark_stmt(&raindex_id);
        let watermark_json = json!([TargetWatermarkRow {
            chain_id: raindex_id.chain_id,
            raindex_address: raindex_id.raindex_address,
            last_block: 42,
            last_hash: Bytes::from_str("0xbeef").unwrap(),
            updated_at: 1,
        }]);

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(&watermark_stmt, watermark_json);

        let state = adapter.inspect_state(&db, &raindex_id).await.unwrap();
        assert!(state.has_required_tables);
        assert_eq!(state.last_synced_block, Some(42));
    }

    #[tokio::test]
    async fn inspect_state_propagates_fetch_tables_error() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default(); // no json for fetch_tables_stmt()
        let raindex_id = RaindexIdentifier {
            chain_id: 1,
            raindex_address: Address::ZERO,
        };
        let err = adapter.inspect_state(&db, &raindex_id).await.unwrap_err();
        match err {
            LocalDbError::LocalDbQueryError(..) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn inspect_state_propagates_watermark_query_error() {
        let adapter = TestBootstrapPipeline::new();
        // Include all required tables so it attempts the watermark query.
        let tables_json = serde_json::to_value(
            REQUIRED_TABLES
                .iter()
                .map(|&name| TableResponse {
                    name: name.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();

        let db = MockDb::default().with_json(&fetch_tables_stmt(), tables_json);
        let raindex_id = RaindexIdentifier {
            chain_id: 1,
            raindex_address: Address::ZERO,
        };
        // Intentionally do not provide json for watermark_stmt -> should error
        let err = adapter.inspect_state(&db, &raindex_id).await.unwrap_err();
        match err {
            LocalDbError::LocalDbQueryError(..) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn reset_db_runs_clear_create_and_insert() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default()
            .with_batch(&clear_tables_batch())
            .with_text(&vacuum_stmt(), "ok")
            .with_batch(&create_tables_batch())
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        adapter
            .reset_db(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        let expected_prefix = reset_prefix_sql();
        assert_eq!(
            calls.len(),
            expected_prefix.len() + 1 + expected_views.len(),
            "unexpected number of executed statements"
        );
        assert_eq!(&calls[..expected_prefix.len()], expected_prefix.as_slice());
        assert_eq!(
            calls[expected_prefix.len()],
            insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string()
        );
        assert_eq!(
            &calls[expected_prefix.len() + 1..],
            expected_views.as_slice()
        );
    }

    #[tokio::test]
    async fn reset_db_uses_default_version_when_none() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default()
            .with_batch(&clear_tables_batch())
            .with_text(&vacuum_stmt(), "ok")
            .with_batch(&create_tables_batch())
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        adapter.reset_db(&db, None).await.unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        let expected_prefix = reset_prefix_sql();
        assert_eq!(&calls[..expected_prefix.len()], expected_prefix.as_slice());
        assert_eq!(
            calls[expected_prefix.len()],
            insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql()
        );
        assert_eq!(
            &calls[expected_prefix.len() + 1..],
            expected_views.as_slice()
        );
    }

    #[tokio::test]
    async fn reset_db_uses_custom_version_when_some() {
        let adapter = TestBootstrapPipeline::new();
        let custom_version = DB_SCHEMA_VERSION + 9;
        let db = MockDb::default()
            .with_batch(&clear_tables_batch())
            .with_text(&vacuum_stmt(), "ok")
            .with_batch(&create_tables_batch())
            .with_text(&insert_db_metadata_stmt(custom_version), "ok")
            .with_views();

        adapter.reset_db(&db, Some(custom_version)).await.unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        let expected_prefix = reset_prefix_sql();
        assert_eq!(&calls[..expected_prefix.len()], expected_prefix.as_slice());
        assert_eq!(
            calls[expected_prefix.len()],
            insert_db_metadata_stmt(custom_version).sql()
        );
        assert_eq!(
            &calls[expected_prefix.len() + 1..],
            expected_views.as_slice()
        );
    }

    #[tokio::test]
    async fn reset_db_propagates_errors() {
        let adapter = TestBootstrapPipeline::new();
        // Only the clear transaction is present; vacuum will fail.
        let db = MockDb::default().with_batch(&clear_tables_batch());

        let err = adapter.reset_db(&db, None).await.unwrap_err();
        match err {
            LocalDbError::LocalDbQueryError(..) => {}
            other => panic!("unexpected error: {other:?}"),
        }

        let calls = db.calls();
        assert_eq!(calls.len(), clear_tables_batch().len() + 1);
        assert_eq!(
            &calls[..clear_tables_batch().len()],
            clear_tables_batch()
                .statements()
                .iter()
                .map(|stmt| stmt.sql().to_string())
                .collect::<Vec<_>>()
                .as_slice()
        );
        assert_eq!(calls.last().unwrap(), vacuum_stmt().sql());
    }

    #[tokio::test]
    async fn engine_run_returns_invalid_bootstrap_implementation() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default();
        let cfg = BootstrapConfig {
            raindex_id: RaindexIdentifier::new(1, Address::ZERO),
            dump_sql: None,
            dump_stmt: None,
            latest_block: 0,
            block_number_threshold: 10_000,
            deployment_block: 1,
        };

        let err = adapter.engine_run(&db, &cfg).await.unwrap_err();
        match err {
            LocalDbError::InvalidBootstrapImplementation => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn runner_run_returns_invalid_bootstrap_implementation() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default();

        let err = adapter.runner_run(&db, None).await.unwrap_err();
        match err {
            LocalDbError::InvalidBootstrapImplementation => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn clear_raindex_data_executes_expected_statement() {
        let adapter = TestBootstrapPipeline::new();
        let raindex_id = RaindexIdentifier::new(42161, Address::from([0x11; 20]));
        let db = RecordingTextExecutor::succeed();

        adapter
            .clear_raindex_data(&db, &raindex_id)
            .await
            .expect("clear_raindex_data should succeed");

        let captured = db.captured_sql();
        let expected: Vec<String> = clear_raindex_data_batch(&raindex_id)
            .statements()
            .iter()
            .map(|stmt| stmt.sql().to_string())
            .collect();
        assert_eq!(captured, expected);
    }

    #[tokio::test]
    async fn clear_raindex_data_propagates_error() {
        let adapter = TestBootstrapPipeline::new();
        let raindex_id = RaindexIdentifier::new(10, Address::from([0x22; 20]));
        let inner_error = LocalDbQueryError::database("boom");
        let db = RecordingTextExecutor::fail(LocalDbError::from(inner_error.clone()));

        let err = adapter
            .clear_raindex_data(&db, &raindex_id)
            .await
            .expect_err("clear_raindex_data should propagate error");

        match err {
            LocalDbError::LocalDbQueryError(actual) => {
                assert_eq!(actual.to_string(), inner_error.to_string());
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let captured = db.captured_sql();
        let expected: Vec<String> = clear_raindex_data_batch(&raindex_id)
            .statements()
            .iter()
            .map(|stmt| stmt.sql().to_string())
            .collect();
        assert_eq!(captured, expected);
    }

    #[tokio::test]
    async fn check_integrity_returns_true_when_ok() {
        use crate::local_db::query::integrity_check::IntegrityCheckRow;

        let adapter = TestBootstrapPipeline::new();
        let row = IntegrityCheckRow {
            quick_check: "ok".to_string(),
        };
        let db = MockDb::default().with_json(&integrity_check_stmt(), json!([row]));

        let is_healthy = adapter.check_integrity(&db).await.unwrap();
        assert!(is_healthy, "should return true for 'ok' response");
    }

    #[tokio::test]
    async fn check_integrity_returns_true_when_ok_uppercase() {
        use crate::local_db::query::integrity_check::IntegrityCheckRow;

        let adapter = TestBootstrapPipeline::new();
        let row = IntegrityCheckRow {
            quick_check: "OK".to_string(),
        };
        let db = MockDb::default().with_json(&integrity_check_stmt(), json!([row]));

        let is_healthy = adapter.check_integrity(&db).await.unwrap();
        assert!(is_healthy, "should return true for 'OK' response");
    }

    #[tokio::test]
    async fn check_integrity_returns_false_when_corrupted() {
        use crate::local_db::query::integrity_check::IntegrityCheckRow;

        let adapter = TestBootstrapPipeline::new();
        let row = IntegrityCheckRow {
            quick_check: "database disk image is malformed".to_string(),
        };
        let db = MockDb::default().with_json(&integrity_check_stmt(), json!([row]));

        let is_healthy = adapter.check_integrity(&db).await.unwrap();
        assert!(
            !is_healthy,
            "should return false for corruption error message"
        );
    }

    #[tokio::test]
    async fn check_integrity_returns_false_for_any_non_ok_response() {
        use crate::local_db::query::integrity_check::IntegrityCheckRow;

        let adapter = TestBootstrapPipeline::new();
        let row = IntegrityCheckRow {
            quick_check: "some error".to_string(),
        };
        let db = MockDb::default().with_json(&integrity_check_stmt(), json!([row]));

        let is_healthy = adapter.check_integrity(&db).await.unwrap();
        assert!(!is_healthy, "should return false for any non-ok response");
    }

    #[tokio::test]
    async fn check_integrity_returns_false_for_empty_response() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default().with_json(
            &integrity_check_stmt(),
            json!([]), // empty array
        );

        let is_healthy = adapter.check_integrity(&db).await.unwrap();
        assert!(!is_healthy, "should return false for empty response");
    }

    #[tokio::test]
    async fn check_integrity_propagates_query_error() {
        let adapter = TestBootstrapPipeline::new();
        let db = MockDb::default(); // no response configured -> will error

        let err = adapter.check_integrity(&db).await.unwrap_err();
        match err {
            LocalDbError::LocalDbQueryError(..) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
