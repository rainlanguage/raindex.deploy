use raindex_common::local_db::{
    pipeline::adapters::bootstrap::{BootstrapConfig, BootstrapPipeline},
    query::LocalDbQueryExecutor,
    LocalDbError,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct ProducerBootstrapAdapter;

impl ProducerBootstrapAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait(?Send)]
impl BootstrapPipeline for ProducerBootstrapAdapter {
    async fn engine_run<DB>(&self, db: &DB, config: &BootstrapConfig) -> Result<(), LocalDbError>
    where
        DB: LocalDbQueryExecutor + ?Sized,
    {
        self.reset_db(db, None).await?;

        if let Some(dump_stmt) = &config.dump_stmt {
            db.execute_batch(dump_stmt).await?;
        }

        Ok(())
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
    use std::sync::Mutex;

    use super::*;
    use alloy::primitives::Address;
    use async_trait::async_trait;
    use raindex_app_settings::local_db_manifest::DB_SCHEMA_VERSION;
    use raindex_common::local_db::query::clear_tables::{clear_tables_batch, vacuum_stmt};
    use raindex_common::local_db::query::create_tables::create_tables_batch;
    use raindex_common::local_db::query::insert_db_metadata::insert_db_metadata_stmt;
    use raindex_common::local_db::query::{
        FromDbJson, LocalDbQueryError, LocalDbQueryExecutor, SqlStatement, SqlStatementBatch,
    };
    use raindex_common::local_db::RaindexIdentifier;

    const TEST_BLOCK_NUMBER_THRESHOLD: u32 = 10_000;

    #[derive(Default)]
    struct MockDb {
        text_map: HashMap<String, String>,
        calls_text: Mutex<Vec<String>>,
    }

    impl MockDb {
        fn with_text(mut self, stmt: &SqlStatement, value: &str) -> Self {
            self.text_map
                .insert(stmt.sql().to_string(), value.to_string());
            self
        }

        fn calls(&self) -> Vec<String> {
            self.calls_text.lock().unwrap().clone()
        }

        fn with_views(self) -> Self {
            raindex_common::local_db::query::create_views::create_views_batch()
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

        fn with_reset_batches(self) -> Self {
            self.with_batch(&clear_tables_batch())
                .with_text(&vacuum_stmt(), "ok")
                .with_batch(&create_tables_batch())
        }
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
        for sql in reset_batch_sqls() {
            assert!(calls.contains(&sql), "missing reset SQL: {sql}");
        }
    }

    fn first_reset_sql() -> String {
        clear_tables_batch().statements()[0].sql().to_string()
    }

    fn last_reset_sql() -> String {
        create_tables_batch()
            .statements()
            .last()
            .unwrap()
            .sql()
            .to_string()
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

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            Err(LocalDbQueryError::database("not supported in these tests"))
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
        RaindexIdentifier::new(1, Address::ZERO)
    }

    #[tokio::test]
    async fn engine_run_resets_and_does_not_import_when_no_dump() {
        let adapter = ProducerBootstrapAdapter::new();
        let db = MockDb::default()
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: None,
            latest_block: 0,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        adapter.engine_run(&db, &cfg).await.unwrap();

        let calls = db.calls();
        let reset_start = first_reset_sql();
        let reset_end = last_reset_sql();
        let insert = insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string();

        assert_reset_batches_were_called(&calls);
        assert!(calls.contains(&insert));

        let idx = |s: &String| calls.iter().position(|c| c == s).unwrap();
        assert!(idx(&reset_start) < idx(&reset_end));
        assert!(idx(&reset_end) < idx(&insert));
    }

    #[tokio::test]
    async fn engine_run_executes_view_creation() {
        let adapter = ProducerBootstrapAdapter::new();
        let db = MockDb::default()
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: None,
            latest_block: 0,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        adapter.engine_run(&db, &cfg).await.unwrap();

        let calls = db.calls();
        let expected_views: Vec<String> =
            raindex_common::local_db::query::create_views::create_views_batch()
                .statements()
                .iter()
                .map(|s| s.sql().to_string())
                .collect();

        for view_stmt in expected_views {
            assert!(calls.contains(&view_stmt), "missing view creation call");
        }
    }

    #[tokio::test]
    async fn engine_run_resets_and_imports_dump_when_present() {
        let adapter = ProducerBootstrapAdapter::new();
        let dump_stmt = SqlStatement::new("--dump-sql");
        let db = MockDb::default()
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_text(&dump_stmt, "ok")
            .with_views();

        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: Some(SqlStatementBatch::from(vec![dump_stmt.clone()])),
            latest_block: 0,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        adapter.engine_run(&db, &cfg).await.unwrap();

        let calls = db.calls();
        let reset_start = first_reset_sql();
        let reset_end = last_reset_sql();
        let insert = insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string();
        let dump = dump_stmt.sql().to_string();

        assert_reset_batches_were_called(&calls);
        assert!(calls.contains(&insert));
        assert!(calls.contains(&dump));

        let idx = |s: &String| calls.iter().position(|c| c == s).unwrap();
        assert!(idx(&reset_start) < idx(&reset_end));
        assert!(idx(&reset_end) < idx(&insert));
        assert!(idx(&insert) < idx(&dump));
    }

    #[tokio::test]
    async fn engine_run_resets_and_fails_when_dump_missing() {
        let adapter = ProducerBootstrapAdapter::new();
        let dump_stmt = SqlStatement::new("--dump-sql-missing");
        let db = MockDb::default()
            .with_reset_batches()
            .with_text(&insert_db_metadata_stmt(DB_SCHEMA_VERSION), "ok")
            .with_views();

        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: Some(SqlStatementBatch::from(vec![dump_stmt.clone()])),
            latest_block: 0,
            block_number_threshold: TEST_BLOCK_NUMBER_THRESHOLD,
            deployment_block: 1,
        };

        // Expect error due to missing dump mapping, after successful reset
        let result = adapter.engine_run(&db, &cfg).await;
        assert!(result.is_err());

        let calls = db.calls();
        let reset_start = first_reset_sql();
        let reset_end = last_reset_sql();
        let insert = insert_db_metadata_stmt(DB_SCHEMA_VERSION).sql().to_string();
        let dump = dump_stmt.sql().to_string();

        assert_reset_batches_were_called(&calls);
        assert!(calls.contains(&insert));
        assert!(calls.contains(&dump));

        let idx = |s: &String| calls.iter().position(|c| c == s).unwrap();
        assert!(idx(&reset_start) < idx(&reset_end));
        assert!(idx(&reset_end) < idx(&insert));
        assert!(idx(&insert) < idx(&dump));
    }

    #[tokio::test]
    async fn engine_run_propagates_reset_error() {
        let adapter = ProducerBootstrapAdapter::new();
        let db = MockDb::default()
            .with_batch(&clear_tables_batch())
            .with_views();

        let cfg = BootstrapConfig {
            raindex_id: sample_ob_id(),
            dump_sql: None,
            dump_stmt: None,
            latest_block: 0,
            block_number_threshold: 1,
            deployment_block: 1,
        };

        let err = adapter.engine_run(&db, &cfg).await.unwrap_err();
        match err {
            LocalDbError::LocalDbQueryError(..) => {}
            other => panic!("unexpected error: {other:?}"),
        }

        let calls = db.calls();
        assert_eq!(calls.len(), clear_tables_batch().len() + 1);
        assert_eq!(calls.last().unwrap(), vacuum_stmt().sql());
    }

    #[tokio::test]
    async fn runner_run_is_unimplemented() {
        let adapter = ProducerBootstrapAdapter::new();
        let db = MockDb::default();

        let err = adapter
            .runner_run(&db, Some(DB_SCHEMA_VERSION))
            .await
            .unwrap_err();
        match err {
            LocalDbError::InvalidBootstrapImplementation => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
