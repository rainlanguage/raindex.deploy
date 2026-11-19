pub use crate::local_db::pipeline::SyncPhase;
use crate::local_db::query::{
    FromDbJson, LocalDbQueryError, LocalDbQueryExecutor, SqlStatement, SqlStatementBatch,
};
use crate::local_db::{LocalDbError, RaindexIdentifier};
#[cfg(target_family = "wasm")]
use executor::JsCallbackExecutor;

use serde_json::Value;
#[cfg(target_family = "wasm")]
use std::rc::Rc;
use std::sync::Arc;
use std::{fmt, future::Future, pin::Pin};
use wasm_bindgen_utils::prelude::*;

#[cfg(target_family = "wasm")]
pub mod executor;
pub mod orders;
pub mod pipeline;
pub mod query;
mod state;
pub(crate) mod status;
pub mod transactions;
pub mod vaults;

pub use state::SyncReadiness;
pub(crate) use state::{ClassifiedChains, LocalDbState, QuerySource};
pub use status::{
    LocalDbStatus, LocalDbStatusSnapshot, LocalDbSyncSnapshot, LocalDbSyncStatusStore,
    NetworkSyncStatus, NetworkSyncStatusSnapshot, RaindexSyncStatus, RaindexSyncStatusSnapshot,
    SchedulerState,
};

#[cfg(target_family = "wasm")]
type ExecuteBatchFn =
    dyn Fn(
        &SqlStatementBatch,
    ) -> Pin<Box<dyn Future<Output = Result<(), LocalDbQueryError>> + 'static>>;
#[cfg(not(target_family = "wasm"))]
type ExecuteBatchFn = dyn Fn(
        &SqlStatementBatch,
    ) -> Pin<Box<dyn Future<Output = Result<(), LocalDbQueryError>> + Send + 'static>>
    + Send
    + Sync;

#[cfg(target_family = "wasm")]
type ExecuteDumpFn =
    dyn Fn(Arc<String>) -> Pin<Box<dyn Future<Output = Result<(), LocalDbQueryError>> + 'static>>;

#[cfg(target_family = "wasm")]
type ExecuteDumpsFn =
    dyn Fn(
        Vec<Arc<String>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), LocalDbQueryError>> + 'static>>;

#[cfg(target_family = "wasm")]
type QueryTextFn =
    dyn Fn(
        &SqlStatement,
    ) -> Pin<Box<dyn Future<Output = Result<String, LocalDbQueryError>> + 'static>>;
#[cfg(not(target_family = "wasm"))]
type QueryTextFn = dyn Fn(
        &SqlStatement,
    ) -> Pin<Box<dyn Future<Output = Result<String, LocalDbQueryError>> + Send + 'static>>
    + Send
    + Sync;

#[cfg(target_family = "wasm")]
type QueryJsonFn =
    dyn Fn(
        &SqlStatement,
    ) -> Pin<Box<dyn Future<Output = Result<Value, LocalDbQueryError>> + 'static>>;
#[cfg(not(target_family = "wasm"))]
type QueryJsonFn = dyn Fn(
        &SqlStatement,
    ) -> Pin<Box<dyn Future<Output = Result<Value, LocalDbQueryError>> + Send + 'static>>
    + Send
    + Sync;

#[cfg(target_family = "wasm")]
type WipeAndRecreateFn =
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), LocalDbQueryError>> + 'static>>;
#[cfg(not(target_family = "wasm"))]
type WipeAndRecreateFn = dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), LocalDbQueryError>> + Send + 'static>>
    + Send
    + Sync;

#[cfg(target_family = "wasm")]
type FnPtr<T> = Rc<T>;
#[cfg(not(target_family = "wasm"))]
type FnPtr<T> = Arc<T>;

#[derive(Clone)]
pub struct LocalDb {
    execute_batch_fn: FnPtr<ExecuteBatchFn>,
    #[cfg(target_family = "wasm")]
    execute_dump_fn: FnPtr<ExecuteDumpFn>,
    #[cfg(target_family = "wasm")]
    execute_dumps_fn: FnPtr<ExecuteDumpsFn>,
    query_text_fn: FnPtr<QueryTextFn>,
    query_json_fn: FnPtr<QueryJsonFn>,
    #[cfg(target_family = "wasm")]
    query_json_retryable_fn: FnPtr<QueryJsonFn>,
    wipe_and_recreate_fn: FnPtr<WipeAndRecreateFn>,
}

#[cfg(target_family = "wasm")]
impl LocalDb {
    pub fn new<E>(executor: E) -> Self
    where
        E: LocalDbQueryExecutor + 'static,
    {
        let exec = Rc::new(executor);

        let execute_batch_fn: FnPtr<ExecuteBatchFn> = {
            let exec = Rc::clone(&exec);
            Rc::new(move |batch: &SqlStatementBatch| {
                let exec = Rc::clone(&exec);
                let batch = batch.clone();
                Box::pin(async move { exec.execute_batch(&batch).await })
            })
        };

        let execute_dump_fn: FnPtr<ExecuteDumpFn> = {
            let exec = Rc::clone(&exec);
            Rc::new(move |dump: Arc<String>| {
                let exec = Rc::clone(&exec);
                Box::pin(async move { exec.execute_sql_dump(dump).await })
            })
        };

        let execute_dumps_fn: FnPtr<ExecuteDumpsFn> = {
            let exec = Rc::clone(&exec);
            Rc::new(move |dumps: Vec<Arc<String>>| {
                let exec = Rc::clone(&exec);
                Box::pin(async move { exec.execute_sql_dumps(dumps).await })
            })
        };

        let query_text_fn: FnPtr<QueryTextFn> = {
            let exec = Rc::clone(&exec);
            Rc::new(move |stmt: &SqlStatement| {
                let exec = Rc::clone(&exec);
                let stmt = stmt.clone();
                Box::pin(async move { exec.query_text(&stmt).await })
            })
        };

        let query_json_fn: FnPtr<QueryJsonFn> = {
            let exec = Rc::clone(&exec);
            Rc::new(move |stmt: &SqlStatement| {
                let exec = Rc::clone(&exec);
                let stmt = stmt.clone();
                Box::pin(async move { exec.query_json::<Value>(&stmt).await })
            })
        };

        let query_json_retryable_fn: FnPtr<QueryJsonFn> = {
            let exec = Rc::clone(&exec);
            Rc::new(move |stmt: &SqlStatement| {
                let exec = Rc::clone(&exec);
                let stmt = stmt.clone();
                Box::pin(async move { exec.query_json_retryable::<Value>(&stmt).await })
            })
        };

        let wipe_and_recreate_fn: FnPtr<WipeAndRecreateFn> = {
            let exec = Rc::clone(&exec);
            Rc::new(move || {
                let exec = Rc::clone(&exec);
                Box::pin(async move { exec.wipe_and_recreate().await })
            })
        };

        Self {
            execute_batch_fn,
            execute_dump_fn,
            execute_dumps_fn,
            query_text_fn,
            query_json_fn,
            query_json_retryable_fn,
            wipe_and_recreate_fn,
        }
    }

    pub(crate) fn from_js_local_db(local_db: JsValue) -> Result<Self, LocalDbQueryError> {
        Ok(Self::new(JsCallbackExecutor::new(local_db)?))
    }
}

#[cfg(not(target_family = "wasm"))]
impl LocalDb {
    pub fn new<E>(executor: E) -> Self
    where
        E: LocalDbQueryExecutor + Send + Sync + 'static,
    {
        let exec = Arc::new(executor);

        let execute_batch_fn: FnPtr<ExecuteBatchFn> = {
            let exec = Arc::clone(&exec);
            Arc::new(move |batch: &SqlStatementBatch| {
                let exec = Arc::clone(&exec);
                let batch = batch.clone();
                Box::pin(async move { exec.execute_batch(&batch).await })
            })
        };

        let query_text_fn: FnPtr<QueryTextFn> = {
            let exec = Arc::clone(&exec);
            Arc::new(move |stmt: &SqlStatement| {
                let exec = Arc::clone(&exec);
                let stmt = stmt.clone();
                Box::pin(async move { exec.query_text(&stmt).await })
            })
        };

        let query_json_fn: FnPtr<QueryJsonFn> = {
            let exec = Arc::clone(&exec);
            Arc::new(move |stmt: &SqlStatement| {
                let exec = Arc::clone(&exec);
                let stmt = stmt.clone();
                Box::pin(async move { exec.query_json::<Value>(&stmt).await })
            })
        };

        let wipe_and_recreate_fn: FnPtr<WipeAndRecreateFn> = {
            let exec = Arc::clone(&exec);
            Arc::new(move || {
                let exec = Arc::clone(&exec);
                Box::pin(async move { exec.wipe_and_recreate().await })
            })
        };

        Self {
            execute_batch_fn,
            query_text_fn,
            query_json_fn,
            wipe_and_recreate_fn,
        }
    }
}

impl fmt::Debug for LocalDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalDb").finish()
    }
}

#[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait::async_trait)]
impl LocalDbQueryExecutor for LocalDb {
    async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
        (self.execute_batch_fn)(batch).await
    }

    #[cfg(target_family = "wasm")]
    async fn execute_sql_dump(&self, dump: Arc<String>) -> Result<(), LocalDbQueryError> {
        (self.execute_dump_fn)(dump).await
    }

    #[cfg(target_family = "wasm")]
    async fn execute_sql_dumps(&self, dumps: Vec<Arc<String>>) -> Result<(), LocalDbQueryError> {
        (self.execute_dumps_fn)(dumps).await
    }

    async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
    where
        T: FromDbJson,
    {
        let value = (self.query_json_fn)(stmt).await?;
        serde_json::from_value(value)
            .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
    }

    #[cfg(target_family = "wasm")]
    async fn query_json_retryable<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
    where
        T: FromDbJson,
    {
        let value = (self.query_json_retryable_fn)(stmt).await?;
        serde_json::from_value(value)
            .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
    }

    async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
        (self.query_text_fn)(stmt).await
    }

    async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
        (self.wipe_and_recreate_fn)().await
    }
}

impl From<LocalDbError> for WasmEncodedError {
    fn from(value: LocalDbError) -> Self {
        WasmEncodedError {
            msg: value.to_string(),
            readable_msg: value.to_readable_msg(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::query::{
        FromDbJson, LocalDbQueryExecutor, SqlStatement, SqlStatementBatch,
    };
    use serde::Deserialize;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
    struct TestRow {
        id: u32,
    }

    #[derive(Clone)]
    struct RecordingExec {
        calls: Arc<Mutex<Vec<&'static str>>>,
        json: String,
        text: String,
    }

    impl RecordingExec {
        fn new(json: impl Into<String>, text: impl Into<String>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                json: json.into(),
                text: text.into(),
            }
        }
    }

    #[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait::async_trait)]
    impl LocalDbQueryExecutor for RecordingExec {
        async fn execute_batch(&self, _batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            self.calls.lock().unwrap().push("batch");
            Ok(())
        }

        async fn query_json<T>(&self, _stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: FromDbJson,
        {
            self.calls.lock().unwrap().push("json");
            serde_json::from_str(&self.json)
                .map_err(|e| LocalDbQueryError::deserialization(e.to_string()))
        }

        async fn query_text(&self, _stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            self.calls.lock().unwrap().push("text");
            Ok(self.text.clone())
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            self.calls.lock().unwrap().push("wipe");
            Ok(())
        }
    }

    #[tokio::test]
    async fn local_db_delegates_to_executor() {
        let exec = RecordingExec::new(r#"[{"id":1}]"#, "ok");
        let db = LocalDb::new(exec.clone());

        db.execute_batch(&SqlStatementBatch::new()).await.unwrap();
        let rows: Vec<TestRow> = db.query_json(&SqlStatement::new("SELECT 1")).await.unwrap();
        let text = db.query_text(&SqlStatement::new("SELECT 2")).await.unwrap();

        assert_eq!(rows, vec![TestRow { id: 1 }]);
        assert_eq!(text, "ok");

        let calls = exec.calls.lock().unwrap().clone();
        assert_eq!(calls, vec!["batch", "json", "text"]);
    }

    #[tokio::test]
    async fn local_db_delegates_wipe_and_recreate_to_executor() {
        let exec = RecordingExec::new("[]", "ok");
        let db = LocalDb::new(exec.clone());

        db.wipe_and_recreate().await.unwrap();

        let calls = exec.calls.lock().unwrap().clone();
        assert_eq!(calls, vec!["wipe"]);
    }

    #[test]
    fn debug_impl_is_stable() {
        let exec = RecordingExec::new("[]", "ok");
        let db = LocalDb::new(exec);
        assert!(format!("{:?}", db).contains("LocalDb"));
    }
}

#[cfg(all(test, target_family = "wasm", feature = "browser-tests"))]
mod wasm_tests {
    use super::*;
    use crate::raindex_client::RaindexClient;
    use gloo_timers::future::TimeoutFuture;
    use raindex_app_settings::spec_version::SpecVersion;
    use raindex_app_settings::yaml::{
        raindex::{RaindexYaml, RaindexYamlValidation},
        YamlParsable,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;
    use wasm_bindgen_utils::{
        prelude::{serde_wasm_bindgen, JsValue},
        result::{WasmEncodedError, WasmEncodedResult},
    };

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn import_preflight_waits_for_cross_tab_ownership() {
        use super::pipeline::runner::leadership::acquire_import;
        use js_sys::{Array, Function, Reflect};
        use std::sync::Arc;
        let mut holder = None;
        for _ in 0..100 {
            holder = acquire_import().await.unwrap();
            if holder.is_some() {
                break;
            }
            TimeoutFuture::new(5).await;
        }
        let holder = holder.expect("previous test must release import ownership");
        let local_db = Function::new_no_args(r#"return {
            reads:0, populated:false, chunks:[],
            query(sql) {
                this.reads++;
                if (!sql.includes('target_watermarks')) throw new Error('must retain populated indexes');
                return {value:this.populated ? '[{"present":1}]' : '[]'};
            },
            transaction() {}, wipeAndRecreate() {},
            beginSqlDumpImport() { return {value:'session'}; },
            appendSqlDumpChunk(id, sql) { this.chunks.push(sql); return {value:''}; },
            finishSqlDumpImport() { return {value:''}; }, cancelSqlDumpImport() {}
        };"#).call0(&JsValue::UNDEFINED).unwrap();
        let exec = JsCallbackExecutor::new(local_db.clone()).unwrap();
        let mut import = exec.execute_sql_dumps(vec![Arc::new(
            "BEGIN; INSERT INTO t VALUES (2); COMMIT;".into(),
        )]);
        for _ in 0..10 {
            assert!(futures::poll!(import.as_mut()).is_pending());
            gloo_timers::future::TimeoutFuture::new(1).await;
        }
        assert_eq!(
            Reflect::get(&local_db, &"reads".into()).unwrap().as_f64(),
            Some(0.0)
        );
        Reflect::set(&local_db, &"populated".into(), &JsValue::TRUE).unwrap();
        drop(holder);
        import.await.unwrap();
        assert_eq!(
            Reflect::get(&local_db, &"reads".into()).unwrap().as_f64(),
            Some(1.0)
        );
        let chunks = Reflect::get(&local_db, &"chunks".into())
            .unwrap()
            .dyn_into::<Array>()
            .unwrap();
        let sql = chunks
            .iter()
            .map(|v| v.as_string().unwrap())
            .collect::<String>();
        assert!(!sql.contains("DROP INDEX"));
    }

    fn single_raindex_settings_yaml() -> String {
        format!(
            r#"
version: {version}
networks:
  anvil:
    rpcs:
      - https://rpc.example/anvil
    chain-id: 42161
subgraphs:
  anvil: https://subgraph.example/anvil
local-db-remotes:
  remote-a: https://manifests.example/a.yaml
local-db-sync:
  anvil:
    batch-size: 10
    max-concurrent-batches: 2
    retry-attempts: 3
    retry-delay-ms: 100
    rate-limit-delay-ms: 1
    finality-depth: 12
    bootstrap-block-threshold: 1000
    sync-interval-ms: 5000
raindexes:
  raindex-a:
    address: 0x00000000000000000000000000000000000000a1
    network: anvil
    subgraph: anvil
    local-db-remote: remote-a
    deployment-block: 123
"#,
            version = SpecVersion::current()
        )
    }

    #[cfg(not(target_family = "wasm"))]
    async fn build_client() -> RaindexClient {
        RaindexClient::new(vec![single_raindex_settings_yaml()], None, None)
            .await
            .expect("valid raindex yaml")
    }

    fn success_callback() -> js_sys::Function {
        let result = WasmEncodedResult::Success::<String> {
            value: "[]".to_string(),
            error: None,
        };
        let js_value = serde_wasm_bindgen::to_value(&result).unwrap();
        js_sys::Function::new_no_args(&format!(
            "return Promise.resolve({})",
            js_sys::JSON::stringify(&js_value)
                .unwrap()
                .as_string()
                .unwrap()
        ))
    }

    fn healthy_db_callback() -> js_sys::Function {
        js_sys::Function::new_with_args(
            "sql",
            r#"
            var value = '[]';
            if (sql && sql.toLowerCase().includes('quick_check')) {
                value = '[{"quick_check":"ok"}]';
            }
            return Promise.resolve({ value: value, error: null });
            "#,
        )
    }

    fn test_local_db(query: js_sys::Function) -> JsValue {
        crate::raindex_client::tests::local_db_object_from_query_callback(query)
    }

    fn recording_status_callback(
        store: Rc<RefCell<Vec<LocalDbStatusSnapshot>>>,
    ) -> js_sys::Function {
        let closure = Closure::wrap(Box::new(move |value: JsValue| {
            if let Ok(snapshot) = serde_wasm_bindgen::from_value::<LocalDbStatusSnapshot>(value) {
                store.borrow_mut().push(snapshot);
            }
        }) as Box<dyn FnMut(JsValue)>);

        let function: js_sys::Function = closure.as_ref().clone().unchecked_into();
        closure.forget();
        function
    }

    #[wasm_bindgen_test]
    async fn local_db_from_js_object_executes_queries() {
        let db = LocalDb::from_js_local_db(test_local_db(success_callback())).unwrap();

        let stmt = SqlStatement::new("SELECT 1");
        let rows: Vec<String> = db.query_json(&stmt).await.unwrap();
        assert!(rows.is_empty());

        let text = db.query_text(&stmt).await.unwrap();
        assert_eq!(text, "[]");
    }

    #[wasm_bindgen_test]
    async fn local_db_from_js_object_surfaces_errors() {
        let error = WasmEncodedResult::Err::<String> {
            value: None,
            error: WasmEncodedError {
                msg: "boom".to_string(),
                readable_msg: "boom readable".to_string(),
            },
        };
        let js_value = serde_wasm_bindgen::to_value(&error).unwrap();
        let callback = js_sys::Function::new_no_args(&format!(
            "return Promise.resolve({})",
            js_sys::JSON::stringify(&js_value)
                .unwrap()
                .as_string()
                .unwrap()
        ));

        let db = LocalDb::from_js_local_db(test_local_db(callback)).unwrap();
        let stmt = SqlStatement::new("SELECT 1");
        let err = db.query_text(&stmt).await.unwrap_err();
        assert!(matches!(err, LocalDbQueryError::Database { .. }));
    }
}
