use crate::local_db::{
    pipeline::adapters::bootstrap::{BootstrapConfig, BootstrapPipeline, BootstrapState},
    query::{
        create_tables::{required_table_schema, REQUIRED_TABLES},
        create_views::create_views_batch,
        fetch_table_columns::{fetch_table_columns_stmt, TableColumnResponse},
        fetch_tables::{fetch_tables_stmt, TableResponse},
        fetch_target_watermark::{fetch_target_watermark_stmt, TargetWatermarkRow},
        LocalDbQueryExecutor, SqlStatement, SqlStatementBatch,
    },
    LocalDbError, RaindexIdentifier,
};
use std::collections::HashSet;
use std::sync::Arc;

const BOOTSTRAP_CACHE_SIZE_SQL: &str = "PRAGMA cache_size = -25000";
#[cfg(target_family = "wasm")]
pub(super) const BOOTSTRAP_CACHE_SIZE_QUERY_SQL: &str =
    "PRAGMA cache_size = -25000; SELECT 1 WHERE 0;";

#[derive(Debug, Default, Clone, Copy)]
pub struct ClientBootstrapAdapter;

impl ClientBootstrapAdapter {
    pub fn new() -> Self {
        Self {}
    }

    async fn fetch_existing_tables<DB>(&self, db: &DB) -> Result<HashSet<String>, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let existing: Vec<TableResponse> = db.query_json_retryable(&fetch_tables_stmt()).await?;
        Ok(existing
            .into_iter()
            .map(|t| t.name.to_ascii_lowercase())
            .collect())
    }

    fn has_required_tables(existing: &HashSet<String>) -> bool {
        REQUIRED_TABLES
            .iter()
            .all(|&t| existing.contains(&t.to_ascii_lowercase()))
    }

    async fn has_required_schema_shape<DB>(
        &self,
        db: &DB,
        existing_tables: &HashSet<String>,
    ) -> Result<bool, LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        for required_table in required_table_schema() {
            if !existing_tables.contains(&required_table.name) {
                return Ok(false);
            }

            let actual_columns: Vec<TableColumnResponse> = db
                .query_json_retryable(&fetch_table_columns_stmt(&required_table.name))
                .await?;
            let actual_column_names: HashSet<String> = actual_columns
                .into_iter()
                .map(|column| column.name.to_ascii_lowercase())
                .collect();

            if required_table
                .columns
                .iter()
                .any(|column| !actual_column_names.contains(column))
            {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn check_threshold(
        &self,
        latest_block: u64,
        last_synced_block: Option<u64>,
        block_number_threshold: u32,
    ) -> Result<(), LocalDbError> {
        if let Some(last_block) = last_synced_block {
            let threshold = u64::from(block_number_threshold);
            if latest_block.saturating_sub(last_block) > threshold {
                return Err(LocalDbError::BlockSyncThresholdExceeded {
                    latest_block,
                    last_indexed_block: last_block,
                    threshold,
                });
            }
        }

        Ok(())
    }

    async fn is_fresh_db<E: LocalDbQueryExecutor + ?Sized>(
        self,
        db: &E,
        raindex_id: &RaindexIdentifier,
    ) -> Result<bool, LocalDbError> {
        let rows: Vec<TargetWatermarkRow> = db
            .query_json_retryable(&fetch_target_watermark_stmt(raindex_id))
            .await?;
        Ok(rows.is_empty())
    }

    async fn apply_dump<DB>(
        &self,
        db: &DB,
        dump_sql: Option<&Arc<String>>,
        dump_stmt: Option<&SqlStatementBatch>,
    ) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        #[cfg(target_family = "wasm")]
        if let Some(dump_sql) = dump_sql {
            // The setter is idempotent, so a follower timeout can be retried.
            // The empty SELECT makes sqlite-web return JSON rather than its
            // text response for statements without result columns. The final
            // semicolon selects the SDK's multi-statement execution path.
            let _: Vec<serde_json::Value> = db
                .query_json_retryable(&SqlStatement::new(BOOTSTRAP_CACHE_SIZE_QUERY_SQL))
                .await?;
            db.execute_sql_dump(Arc::clone(dump_sql))
                .await
                .map_err(LocalDbError::DumpImportFailed)?;
            return Ok(());
        }
        #[cfg(not(target_family = "wasm"))]
        let _ = dump_sql;
        db.query_text(&SqlStatement::new(BOOTSTRAP_CACHE_SIZE_SQL))
            .await?;
        if let Some(dump_stmt) = dump_stmt {
            db.execute_batch(dump_stmt).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl BootstrapPipeline for ClientBootstrapAdapter {
    async fn engine_run<DB>(&self, db: &DB, config: &BootstrapConfig) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let BootstrapState {
            last_synced_block, ..
        } = self.inspect_state(db, &config.raindex_id).await?;

        if config.dump_sql.is_some() || config.dump_stmt.is_some() {
            if self.is_fresh_db(db, &config.raindex_id).await? {
                self.apply_dump(db, config.dump_sql.as_ref(), config.dump_stmt.as_ref())
                    .await?;
                return Ok(());
            }

            match self.check_threshold(
                config.latest_block,
                last_synced_block,
                config.block_number_threshold,
            ) {
                Ok(_) => {}
                Err(_) => {
                    self.clear_raindex_data(db, &config.raindex_id).await?;
                    self.apply_dump(db, config.dump_sql.as_ref(), config.dump_stmt.as_ref())
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn runner_run<DB>(
        &self,
        db: &DB,
        db_schema_version: Option<u32>,
    ) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        let is_healthy = match self.check_integrity(db).await {
            Ok(is_healthy) => is_healthy,
            Err(LocalDbError::LocalDbQueryError(err)) if err.is_worker_unavailable() => {
                return Err(err.into());
            }
            Err(_) => false,
        };
        if !is_healthy {
            db.wipe_and_recreate().await?;
            self.reset_db(db, db_schema_version).await?;
            return Ok(());
        }

        let existing_tables = self.fetch_existing_tables(db).await?;

        if !existing_tables.contains("db_metadata") {
            self.reset_db(db, db_schema_version).await?;
            return Ok(());
        }

        if let Err(err) = self.ensure_schema(db, db_schema_version).await {
            if matches!(
                err,
                LocalDbError::MissingDbMetadataRow | LocalDbError::SchemaVersionMismatch { .. }
            ) {
                self.reset_db(db, db_schema_version).await?;
                return Ok(());
            }

            return Err(err);
        }

        if !Self::has_required_tables(&existing_tables)
            || !self.has_required_schema_shape(db, &existing_tables).await?
        {
            self.reset_db(db, db_schema_version).await?;
        } else {
            db.execute_batch(&create_views_batch()).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_family = "wasm")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn dump_preflight_retries_follower_timeouts() {
        use crate::raindex_client::local_db::executor::JsCallbackExecutor;
        use wasm_bindgen::JsValue;
        use wasm_bindgen_utils::prelude::js_sys::{Function, Object, Reflect};

        let local_db = Function::new_no_args(
            r#"return {
                calls: {},
                query(sql) {
                    this.calls[sql] = (this.calls[sql] || 0) + 1;
                    if (this.calls[sql] === 1) {
                        return {value: undefined, error: {msg: 'timeout', readableMsg: 'Query timeout'}};
                    }
                    if (sql.startsWith('PRAGMA cache_size') &&
                        !(sql.includes('SELECT 1 WHERE 0;') && sql.endsWith(';'))) {
                        return {value: 'Query executed successfully. Rows affected: 0', error: null};
                    }
                    return {value: '[]', error: null};
                },
                transaction() { return {value: '', error: null}; },
                wipeAndRecreate() { throw new Error('must not wipe'); },
                beginSqlDumpImport() { return {value: 'session', error: null}; },
                appendSqlDumpChunk() { return {value: '', error: null}; },
                finishSqlDumpImport() { this.finished = true; return {value: '', error: null}; },
                cancelSqlDumpImport() { throw new Error('must not cancel'); }
            };"#,
        )
        .call0(&JsValue::UNDEFINED)
        .unwrap();
        let db = JsCallbackExecutor::new(local_db.clone()).unwrap();
        let dump = Arc::new("BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string());

        ClientBootstrapAdapter::new()
            .apply_dump(&db, Some(&dump), None)
            .await
            .unwrap();

        let calls = Reflect::get(&local_db, &JsValue::from_str("calls")).unwrap();
        assert_eq!(Object::keys(&calls.clone().into()).length(), 3);
        for sql in [
            BOOTSTRAP_CACHE_SIZE_QUERY_SQL,
            "SELECT 1 AS present FROM target_watermarks LIMIT 1",
            "SELECT m.name, m.sql FROM sqlite_master AS m JOIN pragma_index_list(m.tbl_name) AS i ON i.name = m.name WHERE m.type = 'index' AND m.sql IS NOT NULL AND i.\"unique\" = 0 ORDER BY m.name",
        ] {
            assert_eq!(
                Reflect::get(&calls, &JsValue::from_str(sql))
                    .unwrap()
                    .as_f64(),
                Some(2.0)
            );
        }
        assert_eq!(
            Reflect::get(&local_db, &JsValue::from_str("finished"))
                .unwrap()
                .as_bool(),
            Some(true)
        );
    }

    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::local_db::query::clear_raindex_data::clear_raindex_data_batch;
    use crate::local_db::query::clear_tables::{clear_tables_batch, vacuum_stmt};
    use crate::local_db::query::create_tables::create_tables_batch;
    use crate::local_db::query::create_tables::REQUIRED_TABLES;
    use crate::local_db::query::create_views::create_views_batch;
    use crate::local_db::query::fetch_db_metadata::{fetch_db_metadata_stmt, DbMetadataRow};
    use crate::local_db::query::fetch_tables::{fetch_tables_stmt, TableResponse};
    use crate::local_db::query::fetch_target_watermark::{
        fetch_target_watermark_stmt, TargetWatermarkRow,
    };
    use crate::local_db::query::insert_db_metadata::insert_db_metadata_stmt;
    use crate::local_db::query::FromDbJson;
    use crate::local_db::query::{
        LocalDbQueryError, LocalDbQueryExecutor, SqlStatement, SqlStatementBatch,
    };
    use alloy::primitives::{Address, Bytes};
    use async_trait::async_trait;
    use raindex_app_settings::local_db_manifest::DB_SCHEMA_VERSION;
    use serde_json::json;
    use std::str::FromStr;

    const TEST_BLOCK_NUMBER_THRESHOLD: u32 = 10_000;

    #[derive(Default)]
    struct MockDb {
        json_map: HashMap<String, String>,
        text_map: HashMap<String, String>,
        calls_json: Mutex<Vec<String>>,
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
        fn with_batch(self, batch: &SqlStatementBatch) -> Self {
            batch
                .statements()
                .iter()
                .fold(self, |db, stmt| db.with_text(stmt, "ok"))
        }
        fn with_reset_batches(self) -> Self {
            self.with_batch(&clear_tables_batch())
                .with_text(&vacuum_stmt(), "ok")
                .with_batch(&create_tables_batch())
        }
        fn calls(&self) -> Vec<String> {
            self.calls_text.lock().unwrap().clone()
        }
        fn json_calls(&self) -> Vec<String> {
            self.calls_json.lock().unwrap().clone()
        }

        fn with_views(self) -> Self {
            create_views_batch()
                .statements()
                .iter()
                .fold(self, |db, stmt| db.with_text(stmt, "ok"))
        }

        fn with_healthy_integrity(self) -> Self {
            use crate::local_db::query::integrity_check::{
                integrity_check_stmt, IntegrityCheckRow,
            };
            let row = IntegrityCheckRow {
                quick_check: "ok".to_string(),
            };
            self.with_json(&integrity_check_stmt(), json!([row]))
        }

        fn with_required_schema_columns(self) -> Self {
            required_table_schema().into_iter().fold(self, |db, table| {
                let rows = table
                    .columns
                    .iter()
                    .map(|name| TableColumnResponse { name: name.clone() })
                    .collect::<Vec<_>>();
                db.with_json(&fetch_table_columns_stmt(&table.name), json!(rows))
            })
        }

        fn with_required_schema_columns_missing(self, table_name: &str, column_name: &str) -> Self {
            required_table_schema().into_iter().fold(self, |db, table| {
                let rows = table
                    .columns
                    .iter()
                    .filter(|&name| table.name != table_name || name != column_name)
                    .map(|name| TableColumnResponse { name: name.clone() })
                    .collect::<Vec<_>>();
                db.with_json(&fetch_table_columns_stmt(&table.name), json!(rows))
            })
        }
    }

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
            self.calls_json.lock().unwrap().push(sql.to_string());
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

    fn sample_ob_id() -> RaindexIdentifier {
        RaindexIdentifier {
            chain_id: 1,
            raindex_address: Address::ZERO,
        }
    }

    fn runner_ob_id() -> RaindexIdentifier {
        RaindexIdentifier::new(0, Address::ZERO)
    }

    fn cfg_with_dump(latest_block: u64) -> BootstrapConfig {
        BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: Some(SqlStatementBatch::from(vec![SqlStatement::new(
                "--dump-sql",
            )])),
            latest_block,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        }
    }

    fn required_tables_json() -> serde_json::Value {
        serde_json::to_value(
            REQUIRED_TABLES
                .iter()
                .map(|&t| TableResponse {
                    name: t.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn table_names_json(names: &[&str]) -> serde_json::Value {
        serde_json::to_value(
            names
                .iter()
                .map(|&name| TableResponse {
                    name: name.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn required_tables_without_db_metadata_json() -> serde_json::Value {
        serde_json::to_value(
            REQUIRED_TABLES
                .iter()
                .filter(|&&name| name != "db_metadata")
                .map(|&name| TableResponse {
                    name: name.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn reset_batch_sqls() -> Vec<String> {
        let mut statements = clear_tables_batch().statements().to_vec();
        statements.push(vacuum_stmt());
        statements.extend(create_tables_batch().statements().iter().cloned());
        statements
            .iter()
            .map(|stmt| stmt.sql().to_string())
            .collect()
    }

    fn assert_reset_batches_were_called(calls: &[String]) {
        let expected = reset_batch_sqls();
        for sql in expected {
            assert!(calls.contains(&sql), "missing reset SQL: {sql}");
        }
    }

    fn insert_reset_text_map(text_map: &mut HashMap<String, String>) {
        for sql in reset_batch_sqls() {
            text_map.insert(sql, "ok".to_string());
        }
    }

    fn watermark_row(last_block: u64) -> TargetWatermarkRow {
        TargetWatermarkRow {
            chain_id: sample_ob_id().chain_id,
            raindex_address: sample_ob_id().raindex_address,
            last_block,
            last_hash: Bytes::from_str("0xbeef").unwrap(),
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn runner_run_resets_when_tables_missing() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = json!([]);
        let db_meta_row = DbMetadataRow {
            id: 1,
            db_schema_version: DB_SCHEMA_VERSION,
            created_at: None,
            updated_at: None,
        };

        let db = MockDb::default()
            .with_healthy_integrity()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(&fetch_db_metadata_stmt(), json!([db_meta_row]))
            .with_required_schema_columns()
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();
        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        let expected_reset = reset_batch_sqls();
        assert_eq!(&calls[..expected_reset.len()], expected_reset.as_slice());
        assert_eq!(
            calls[expected_reset.len()],
            insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string()
        );
        assert_eq!(
            &calls[expected_reset.len() + 1..],
            expected_views.as_slice()
        );
    }

    #[tokio::test]
    async fn runner_run_resets_on_missing_db_metadata() {
        let adapter = ClientBootstrapAdapter::new();

        let db = MockDb::default()
            .with_healthy_integrity()
            .with_json(&fetch_tables_stmt(), table_names_json(&["db_metadata"]))
            .with_json(&fetch_db_metadata_stmt(), json!([])) // triggers reset
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();
        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        assert_reset_batches_were_called(&calls);
        assert!(calls.contains(&insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string()));
        assert!(
            expected_views.iter().all(|stmt| calls.contains(stmt)),
            "missing view creation statements"
        );
        assert!(
            !db.json_calls().contains(
                &fetch_target_watermark_stmt(&runner_ob_id())
                    .sql()
                    .to_string()
            ),
            "schema recovery should not inspect target watermarks before reset"
        );
    }

    #[tokio::test]
    async fn runner_run_resets_on_missing_db_metadata_table() {
        let adapter = ClientBootstrapAdapter::new();

        let db = MockDb::default()
            .with_healthy_integrity()
            .with_json(
                &fetch_tables_stmt(),
                required_tables_without_db_metadata_json(),
            )
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        assert_reset_batches_were_called(&calls);
        assert!(calls.contains(&insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string()));
        assert!(
            expected_views.iter().all(|stmt| calls.contains(stmt)),
            "missing view creation statements"
        );
        assert!(
            !db.json_calls()
                .contains(&fetch_db_metadata_stmt().sql().to_string()),
            "missing metadata table should reset before querying db_metadata"
        );
        assert!(
            !db.json_calls().contains(
                &fetch_target_watermark_stmt(&runner_ob_id())
                    .sql()
                    .to_string()
            ),
            "missing metadata table should reset before target watermark inspection"
        );
    }

    #[tokio::test]
    async fn runner_run_resets_on_schema_mismatch() {
        let adapter = ClientBootstrapAdapter::new();

        let mismatched_row = DbMetadataRow {
            id: 1,
            db_schema_version: DB_SCHEMA_VERSION + 1,
            created_at: None,
            updated_at: None,
        };

        let db = MockDb::default()
            .with_healthy_integrity()
            .with_json(&fetch_tables_stmt(), table_names_json(&["db_metadata"]))
            .with_json(&fetch_db_metadata_stmt(), json!([mismatched_row]))
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        assert_reset_batches_were_called(&calls);
        assert!(
            expected_views.iter().all(|stmt| calls.contains(stmt)),
            "missing view creation statements"
        );
        assert!(
            !db.json_calls().contains(
                &fetch_target_watermark_stmt(&runner_ob_id())
                    .sql()
                    .to_string()
            ),
            "schema recovery should not inspect target watermarks before reset"
        );
    }

    #[tokio::test]
    async fn runner_run_resets_old_schema_before_watermark_query() {
        let adapter = ClientBootstrapAdapter::new();
        let old_schema_row = DbMetadataRow {
            id: 1,
            db_schema_version: DB_SCHEMA_VERSION - 1,
            created_at: None,
            updated_at: None,
        };

        let db = MockDb::default()
            .with_healthy_integrity()
            .with_json(&fetch_tables_stmt(), table_names_json(&["db_metadata"]))
            .with_json(&fetch_db_metadata_stmt(), json!([old_schema_row]))
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        assert_reset_batches_were_called(&calls);
        assert!(calls.contains(&insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string()));
        assert!(
            expected_views.iter().all(|stmt| calls.contains(stmt)),
            "missing view creation statements"
        );
        assert!(
            !db.json_calls().contains(
                &fetch_target_watermark_stmt(&runner_ob_id())
                    .sql()
                    .to_string()
            ),
            "old schema should reset before preparing current-schema watermark SQL"
        );
    }

    #[tokio::test]
    async fn runner_run_is_idempotent_when_schema_ok() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = serde_json::to_value(
            REQUIRED_TABLES
                .iter()
                .map(|&t| TableResponse {
                    name: t.to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();

        let db_row = DbMetadataRow {
            id: 1,
            db_schema_version: DB_SCHEMA_VERSION,
            created_at: None,
            updated_at: None,
        };

        let db = MockDb::default()
            .with_healthy_integrity()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(&fetch_db_metadata_stmt(), json!([db_row]))
            .with_required_schema_columns()
            .with_json(&fetch_target_watermark_stmt(&runner_ob_id()), json!([]))
            .with_views();

        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        assert_eq!(
            db.calls(),
            expected_views,
            "schema-ok bootstrap should still refresh replaceable views"
        );
    }

    #[tokio::test]
    async fn runner_run_resets_when_required_column_is_missing() {
        let adapter = ClientBootstrapAdapter::new();
        let db_row = DbMetadataRow {
            id: 1,
            db_schema_version: DB_SCHEMA_VERSION,
            created_at: None,
            updated_at: None,
        };

        let db = MockDb::default()
            .with_healthy_integrity()
            .with_json(&fetch_tables_stmt(), required_tables_json())
            .with_json(&fetch_db_metadata_stmt(), json!([db_row]))
            .with_required_schema_columns_missing("target_watermarks", "raindex_address")
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        assert_reset_batches_were_called(&calls);
        assert!(calls.contains(&insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string()));
        assert!(
            expected_views.iter().all(|stmt| calls.contains(stmt)),
            "missing view creation statements"
        );
        assert!(
            !db.json_calls().contains(
                &fetch_target_watermark_stmt(&runner_ob_id())
                    .sql()
                    .to_string()
            ),
            "invalid schema shape should reset before target watermark inspection"
        );
    }

    #[tokio::test]
    async fn runner_run_propagates_unexpected_ensure_schema_error() {
        let adapter = ClientBootstrapAdapter::new();

        let db = MockDb::default().with_healthy_integrity();

        let err = adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap_err();

        match err {
            LocalDbError::LocalDbQueryError(..) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn engine_run_applies_dump_on_fresh_db() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = required_tables_json();
        let dump_stmt = SqlStatement::new("--dump-sql");
        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: Some(SqlStatementBatch::from(vec![dump_stmt.clone()])),
            latest_block: 100,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(&fetch_target_watermark_stmt(&cfg.raindex_id), json!([]))
            .with_text(&SqlStatement::new(BOOTSTRAP_CACHE_SIZE_SQL), "ok")
            .with_text(&dump_stmt, "ok");

        adapter.engine_run(&db, &cfg).await.unwrap();

        assert_eq!(
            db.calls(),
            vec![
                BOOTSTRAP_CACHE_SIZE_SQL.to_string(),
                dump_stmt.sql().to_string()
            ]
        );
    }

    #[tokio::test]
    async fn engine_run_clears_and_applies_dump_when_threshold_exceeded() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = required_tables_json();
        let last_synced = 50_000u64;
        let latest = last_synced + u64::from(TEST_BLOCK_NUMBER_THRESHOLD) + 1;
        let dump_stmt = SqlStatement::new("--dump-sql");
        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: Some(SqlStatementBatch::from(vec![dump_stmt.clone()])),
            latest_block: latest,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        let clear_batch = clear_raindex_data_batch(&sample_ob_id());
        let mut db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(
                &fetch_target_watermark_stmt(&cfg.raindex_id),
                json!([watermark_row(last_synced)]),
            )
            .with_text(&dump_stmt, "dumped");

        for stmt in clear_batch.statements() {
            db = db.with_text(stmt, "cleared");
        }
        db = db.with_text(&SqlStatement::new(BOOTSTRAP_CACHE_SIZE_SQL), "ok");

        adapter.engine_run(&db, &cfg).await.unwrap();

        let calls = db.calls();
        let mut expected: Vec<String> = clear_batch
            .statements()
            .iter()
            .map(|stmt| stmt.sql().to_string())
            .collect();
        expected.push(BOOTSTRAP_CACHE_SIZE_SQL.to_string());
        expected.push(dump_stmt.sql().to_string());
        assert_eq!(calls, expected);
    }

    #[tokio::test]
    async fn engine_run_skips_actions_within_threshold() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = required_tables_json();
        let last_synced = 100_000u64;
        let latest = last_synced + u64::from(TEST_BLOCK_NUMBER_THRESHOLD) - 1;
        let cfg = cfg_with_dump(latest);

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(
                &fetch_target_watermark_stmt(&cfg.raindex_id),
                json!([watermark_row(last_synced)]),
            );

        adapter.engine_run(&db, &cfg).await.unwrap();

        assert!(db.calls().is_empty());
    }

    #[tokio::test]
    async fn engine_run_skips_actions_at_threshold_boundary() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = required_tables_json();
        let last_synced = 120_000u64;
        let latest = last_synced + u64::from(TEST_BLOCK_NUMBER_THRESHOLD);
        let cfg = cfg_with_dump(latest);

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(
                &fetch_target_watermark_stmt(&cfg.raindex_id),
                json!([watermark_row(last_synced)]),
            );

        adapter.engine_run(&db, &cfg).await.unwrap();

        assert!(db.calls().is_empty());
    }

    #[tokio::test]
    async fn engine_run_does_nothing_without_dump_even_when_threshold_exceeded() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = required_tables_json();
        let last_synced = 200_000u64;
        let latest = last_synced + u64::from(TEST_BLOCK_NUMBER_THRESHOLD) + 5;
        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: None,
            latest_block: latest,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(
                &fetch_target_watermark_stmt(&cfg.raindex_id),
                json!([watermark_row(last_synced)]),
            );

        adapter.engine_run(&db, &cfg).await.unwrap();

        assert!(db.calls().is_empty());
    }

    #[tokio::test]
    async fn engine_run_propagates_clear_errors() {
        let adapter = ClientBootstrapAdapter::new();
        let tables_json = required_tables_json();
        let last_synced = 80_000u64;
        let latest = last_synced + u64::from(TEST_BLOCK_NUMBER_THRESHOLD) + 42;
        let dump_stmt = SqlStatement::new("--dump-sql");
        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: Some(SqlStatementBatch::from(vec![dump_stmt.clone()])),
            latest_block: latest,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        let db = MockDb::default()
            .with_json(&fetch_tables_stmt(), tables_json)
            .with_json(
                &fetch_target_watermark_stmt(&cfg.raindex_id),
                json!([watermark_row(last_synced)]),
            );

        let err = adapter.engine_run(&db, &cfg).await.unwrap_err();
        match err {
            LocalDbError::LocalDbQueryError(..) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    struct CorruptedDb {
        json_map: HashMap<String, String>,
        text_map: HashMap<String, String>,
        calls_text: Mutex<Vec<String>>,
        wipe_called: Mutex<bool>,
    }

    impl CorruptedDb {
        fn new() -> Self {
            use crate::local_db::query::integrity_check::{
                integrity_check_stmt, IntegrityCheckRow,
            };

            let mut db = Self {
                json_map: HashMap::new(),
                text_map: HashMap::new(),
                calls_text: Mutex::new(Vec::new()),
                wipe_called: Mutex::new(false),
            };
            let corrupted_row = IntegrityCheckRow {
                quick_check: "database disk image is malformed".to_string(),
            };
            db.json_map.insert(
                integrity_check_stmt().sql().to_string(),
                json!([corrupted_row]).to_string(),
            );
            insert_reset_text_map(&mut db.text_map);
            db.text_map.insert(
                insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string(),
                "ok".to_string(),
            );
            for stmt in create_views_batch().statements() {
                db.text_map.insert(stmt.sql().to_string(), "ok".to_string());
            }
            db
        }

        fn calls(&self) -> Vec<String> {
            self.calls_text.lock().unwrap().clone()
        }

        fn was_wipe_called(&self) -> bool {
            *self.wipe_called.lock().unwrap()
        }
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for CorruptedDb {
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
            self.text_map
                .get(sql)
                .cloned()
                .ok_or_else(|| LocalDbQueryError::database("no text for sql"))
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            *self.wipe_called.lock().unwrap() = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn runner_run_resets_on_corrupted_database() {
        let adapter = ClientBootstrapAdapter::new();
        let db = CorruptedDb::new();

        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .expect("runner_run should succeed after resetting corrupted db");

        assert!(db.was_wipe_called(), "should have called wipe_and_recreate");

        let calls = db.calls();
        let expected_views: Vec<String> = create_views_batch()
            .statements()
            .iter()
            .map(|s| s.sql().to_string())
            .collect();
        assert_reset_batches_were_called(&calls);
        assert!(
            calls.contains(&insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string()),
            "should have inserted db metadata"
        );
        assert!(
            expected_views.iter().all(|stmt| calls.contains(stmt)),
            "missing view creation statements"
        );
    }

    struct IntegrityCheckFailsDb {
        query_error: &'static str,
        text_map: HashMap<String, String>,
        calls_text: Mutex<Vec<String>>,
        wipe_called: Mutex<bool>,
    }

    impl IntegrityCheckFailsDb {
        fn new() -> Self {
            let mut db = Self {
                query_error: "malformed database schema (db_metadata)",
                text_map: HashMap::new(),
                calls_text: Mutex::new(Vec::new()),
                wipe_called: Mutex::new(false),
            };
            insert_reset_text_map(&mut db.text_map);
            db.text_map.insert(
                insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string(),
                "ok".to_string(),
            );
            for stmt in create_views_batch().statements() {
                db.text_map.insert(stmt.sql().to_string(), "ok".to_string());
            }
            db
        }

        fn calls(&self) -> Vec<String> {
            self.calls_text.lock().unwrap().clone()
        }

        fn was_wipe_called(&self) -> bool {
            *self.wipe_called.lock().unwrap()
        }
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for IntegrityCheckFailsDb {
        async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            for stmt in batch {
                let _ = self.query_text(stmt).await?;
            }
            Ok(())
        }

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            Err(LocalDbQueryError::database(self.query_error))
        }

        async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            let sql = stmt.sql();
            self.calls_text.lock().unwrap().push(sql.to_string());
            self.text_map
                .get(sql)
                .cloned()
                .ok_or_else(|| LocalDbQueryError::database("no text for sql"))
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            *self.wipe_called.lock().unwrap() = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn runner_run_resets_when_integrity_check_fails_with_error() {
        let adapter = ClientBootstrapAdapter::new();
        let db = IntegrityCheckFailsDb::new();

        adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .expect("runner_run should succeed after resetting when integrity check errors");

        assert!(
            db.was_wipe_called(),
            "should have called wipe_and_recreate when integrity check errors"
        );

        let calls = db.calls();
        assert_reset_batches_were_called(&calls);
    }

    #[tokio::test]
    async fn runner_run_keeps_database_when_worker_is_unavailable() {
        let adapter = ClientBootstrapAdapter::new();
        for query_error in [
            "JavaScript error: JsValue(\"SQL dump import is in progress.\")",
            "JavaScript error: JsValue(\"Query timeout\")",
            "Initialization pending",
        ] {
            let mut db = IntegrityCheckFailsDb::new();
            db.query_error = query_error;

            let err = adapter
                .runner_run(&db, Some(DB_SCHEMA_VERSION))
                .await
                .unwrap_err();

            assert!(matches!(
                err,
                LocalDbError::LocalDbQueryError(ref query_err)
                    if query_err.is_worker_unavailable()
            ));
            assert!(!db.was_wipe_called(), "wiped on {query_error}");
            assert!(db.calls().is_empty());
        }
    }

    struct WipeFailsDb;

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for WipeFailsDb {
        async fn execute_batch(&self, _batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            Ok(())
        }

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            Err(LocalDbQueryError::database(
                "malformed database schema (db_metadata)",
            ))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            Ok("ok".to_string())
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::database("wipe failed"))
        }
    }

    #[tokio::test]
    async fn runner_run_propagates_wipe_error() {
        let adapter = ClientBootstrapAdapter::new();
        let db = WipeFailsDb;

        let err = adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap_err();

        match err {
            LocalDbError::LocalDbQueryError(inner) => {
                assert!(inner.to_string().contains("wipe failed"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
