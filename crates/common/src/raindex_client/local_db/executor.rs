use super::*;
use crate::local_db::query::{
    FromDbJson, LocalDbQueryError, LocalDbQueryExecutor, SqlStatement, SqlStatementBatch, SqlValue,
};
use async_trait::async_trait;
use futures::lock::{Mutex, OwnedMutexGuard};
use js_sys::{Array, BigInt, Object, Reflect};
use std::future::Future;
#[cfg(test)]
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen_utils::prelude::wasm_bindgen_futures::JsFuture;
use wasm_bindgen_utils::prelude::JsCast;
use wasm_bindgen_utils::result::WasmEncodedResult;

#[derive(Clone)]
pub struct JsCallbackExecutor {
    local_db: JsValue,
    query_callback: js_sys::Function,
    wipe_callback: js_sys::Function,
    transaction_callback: js_sys::Function,
    begin_import_callback: js_sys::Function,
    append_import_callback: js_sys::Function,
    finish_import_callback: js_sys::Function,
    cancel_import_callback: js_sys::Function,
    serialize: Arc<Mutex<()>>,
}

impl JsCallbackExecutor {
    pub fn new(local_db: JsValue) -> Result<Self, LocalDbQueryError> {
        let query_callback = method(&local_db, "query")?;
        let wipe_callback = method(&local_db, "wipeAndRecreate")?;
        let transaction_callback = method(&local_db, "transaction")?;
        let begin_import_callback = method(&local_db, "beginSqlDumpImport")?;
        let append_import_callback = method(&local_db, "appendSqlDumpChunk")?;
        let finish_import_callback = method(&local_db, "finishSqlDumpImport")?;
        let cancel_import_callback = method(&local_db, "cancelSqlDumpImport")?;

        Ok(Self {
            local_db,
            query_callback,
            wipe_callback,
            transaction_callback,
            begin_import_callback,
            append_import_callback,
            finish_import_callback,
            cancel_import_callback,
            serialize: Arc::new(Mutex::new(())),
        })
    }

    pub fn from_ref(query_callback: &js_sys::Function) -> Self {
        let local_db = Object::new();
        Reflect::set(&local_db, &JsValue::from_str("query"), query_callback).unwrap();
        Self {
            local_db: local_db.into(),
            query_callback: query_callback.clone(),
            wipe_callback: js_sys::Function::new_no_args("return undefined"),
            transaction_callback: js_sys::Function::new_no_args("return undefined"),
            begin_import_callback: js_sys::Function::new_no_args("return undefined"),
            append_import_callback: js_sys::Function::new_no_args("return undefined"),
            finish_import_callback: js_sys::Function::new_no_args("return undefined"),
            cancel_import_callback: js_sys::Function::new_no_args("return undefined"),
            serialize: Arc::new(Mutex::new(())),
        }
    }

    fn function(&self) -> &js_sys::Function {
        &self.query_callback
    }

    async fn invoke_statement_unlocked(
        &self,
        stmt: &SqlStatement,
    ) -> Result<String, LocalDbQueryError> {
        wait_while_busy(LocalDbQueryError::is_import_in_progress, || {
            self.call_statement_unlocked(stmt)
        })
        .await
    }

    async fn call_statement_unlocked(
        &self,
        stmt: &SqlStatement,
    ) -> Result<String, LocalDbQueryError> {
        // If there are no parameters, pass `undefined` to the JS callback
        // instead of an empty array to match the SDK's expected semantics.
        let js_params_val = if stmt.params().is_empty() {
            JsValue::UNDEFINED
        } else {
            sql_params_to_js(stmt.params())
        };

        let result = self
            .function()
            .call2(
                &self.local_db,
                &JsValue::from_str(stmt.sql()),
                &js_params_val,
            )
            .map_err(|e| {
                LocalDbQueryError::database(format!(
                    "JavaScript callback invocation failed: {:?}",
                    e
                ))
            })?;

        let promise = js_sys::Promise::resolve(&result);
        let future = JsFuture::from(promise);
        let js_result = future.await.map_err(|e| {
            LocalDbQueryError::database(format!("Promise resolution failed: {:?}", e))
        })?;

        decode_string_result(js_result)
    }

    async fn invoke_statement(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
        let _guard = self.serialize.lock().await;
        self.invoke_statement_unlocked(stmt).await
    }

    /// A follower's read can also time out behind a long import job. Only
    /// reads retry on that, because a timed-out write may still commit.
    async fn invoke_read(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
        let _guard = self.serialize.lock().await;
        self.invoke_read_unlocked(stmt).await
    }

    async fn invoke_read_unlocked(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
        wait_while_busy(LocalDbQueryError::is_worker_unavailable, || {
            self.call_statement_unlocked(stmt)
        })
        .await
    }

    async fn invoke_transaction_unlocked(
        &self,
        batch: &SqlStatementBatch,
    ) -> Result<(), LocalDbQueryError> {
        wait_while_busy(LocalDbQueryError::is_import_in_progress, || {
            self.call_transaction_unlocked(batch)
        })
        .await
    }

    async fn call_transaction_unlocked(
        &self,
        batch: &SqlStatementBatch,
    ) -> Result<(), LocalDbQueryError> {
        let statements = Array::new();
        batch.inner_statements().iter().try_for_each(|stmt| {
            statements.push(&transaction_statement(stmt.sql(), stmt.params())?.into());
            Ok::<(), LocalDbQueryError>(())
        })?;

        let result = self
            .transaction_callback
            .call1(&self.local_db, &statements)
            .map_err(|e| {
                LocalDbQueryError::database(format!(
                    "JavaScript transaction callback invocation failed: {:?}",
                    e
                ))
            })?;

        let promise = js_sys::Promise::resolve(&result);
        let future = JsFuture::from(promise);
        let js_result = future.await.map_err(|e| {
            LocalDbQueryError::database(format!("Transaction promise resolution failed: {:?}", e))
        })?;

        decode_string_result(js_result).map(|_| ())
    }

    fn import_promise(
        &self,
        callback: &js_sys::Function,
        args: ImportArgs<'_>,
    ) -> Result<js_sys::Promise, LocalDbQueryError> {
        let result = match args {
            ImportArgs::Begin => callback.call0(&self.local_db),
            ImportArgs::Session(id) => callback.call1(&self.local_db, &JsValue::from_str(id)),
            ImportArgs::Chunk(id, sql) => callback.call2(
                &self.local_db,
                &JsValue::from_str(id),
                &JsValue::from_str(sql),
            ),
        }
        .map_err(|e| {
            LocalDbQueryError::database(format!("JavaScript import callback failed: {e:?}"))
        })?;
        Ok(js_sys::Promise::resolve(&result))
    }

    async fn invoke_import_unlocked(
        &self,
        callback: &js_sys::Function,
        args: ImportArgs<'_>,
    ) -> Result<String, LocalDbQueryError> {
        let js_result = JsFuture::from(self.import_promise(callback, args)?)
            .await
            .map_err(|e| LocalDbQueryError::database(format!("Import promise failed: {e:?}")))?;
        decode_string_result(js_result)
    }

    async fn append_import_sql(
        &self,
        guard: &mut ImportSessionGuard,
        id: &str,
        sql: &str,
    ) -> Result<(), LocalDbQueryError> {
        const CHUNK_BYTES: usize = 256 * 1024;
        let mut start = 0;
        while start < sql.len() {
            let mut end = (start + CHUNK_BYTES).min(sql.len());
            while !sql.is_char_boundary(end) {
                end -= 1;
            }
            guard
                .invoke(
                    &self.append_import_callback,
                    ImportArgs::Chunk(id, &sql[start..end]),
                )
                .await?;
            start = end;
        }
        Ok(())
    }
}

struct ImportSessionGuard {
    executor: JsCallbackExecutor,
    pending: Option<PendingImport>,
    serialize: Option<OwnedMutexGuard<()>>,
    ownership: Option<super::pipeline::runner::leadership::LeadershipGuard>,
}

enum PendingImport {
    Beginning(js_sys::Promise),
    Active {
        id: String,
        in_flight: Option<js_sys::Promise>,
    },
}

impl ImportSessionGuard {
    async fn invoke(
        &mut self,
        callback: &js_sys::Function,
        args: ImportArgs<'_>,
    ) -> Result<String, LocalDbQueryError> {
        let promise = self.executor.import_promise(callback, args)?;
        if let Some(PendingImport::Active { in_flight, .. }) = &mut self.pending {
            *in_flight = Some(promise.clone());
        }
        let outcome = JsFuture::from(promise).await;
        if let Some(PendingImport::Active { in_flight, .. }) = &mut self.pending {
            *in_flight = None;
        }
        decode_string_result(
            outcome.map_err(|e| {
                LocalDbQueryError::database(format!("Import promise failed: {e:?}"))
            })?,
        )
    }

    fn disarm(&mut self) {
        self.pending = None;
    }
}

impl Drop for ImportSessionGuard {
    fn drop(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let executor = self.executor.clone();
        let serialize = self.serialize.take();
        let ownership = self.ownership.take();
        wasm_bindgen_utils::prelude::wasm_bindgen_futures::spawn_local(async move {
            // Keep later queries/imports serialized until rollback finishes.
            let _serialize = serialize;
            let _ownership = ownership;
            let id = match pending {
                PendingImport::Active { id, in_flight } => {
                    // The SDK rejects cancellation while an append/finish/cancel
                    // request is still in flight on this connection.
                    if let Some(promise) = in_flight {
                        let _ = JsFuture::from(promise).await;
                    }
                    Some(id)
                }
                PendingImport::Beginning(promise) => JsFuture::from(promise)
                    .await
                    .ok()
                    .and_then(|value| decode_string_result(value).ok()),
            };
            if let Some(id) = id {
                let _ = executor
                    .invoke_import_unlocked(
                        &executor.cancel_import_callback,
                        ImportArgs::Session(&id),
                    )
                    .await;
            }
        });
    }
}

enum ImportArgs<'a> {
    Begin,
    Session(&'a str),
    Chunk(&'a str, &'a str),
}

/// Another tab's import can hold the shared worker for many seconds. The SDK
/// expires an abandoned import after two minutes of inactivity.
const IMPORT_WAIT_POLL_MS: u32 = 250;
const IMPORT_WAIT_TIMEOUT_MS: f64 = 300_000.0;

async fn wait_while_busy<T, F, Fut>(
    is_busy: fn(&LocalDbQueryError) -> bool,
    mut request: F,
) -> Result<T, LocalDbQueryError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LocalDbQueryError>>,
{
    let started = js_sys::Date::now();
    loop {
        match request().await {
            Err(err) if is_busy(&err) && within_import_wait(started) => {
                gloo_timers::future::TimeoutFuture::new(IMPORT_WAIT_POLL_MS).await;
            }
            result => return result,
        }
    }
}

fn within_import_wait(started_ms: f64) -> bool {
    js_sys::Date::now() - started_ms < IMPORT_WAIT_TIMEOUT_MS
}

fn decode_string_result(value: JsValue) -> Result<String, LocalDbQueryError> {
    let result: WasmEncodedResult<String> =
        serde_wasm_bindgen::from_value(value).map_err(|_| LocalDbQueryError::invalid_response())?;
    match result {
        WasmEncodedResult::Success { value, .. } => Ok(value),
        WasmEncodedResult::Err { error, .. } => {
            Err(LocalDbQueryError::database(error.readable_msg))
        }
    }
}

#[derive(serde::Deserialize)]
struct IndexDefinition {
    name: String,
    sql: String,
}

fn transaction_statement(sql: &str, params: &[SqlValue]) -> Result<Object, LocalDbQueryError> {
    let item = Object::new();
    Reflect::set(&item, &JsValue::from_str("sql"), &JsValue::from_str(sql))
        .map_err(|e| LocalDbQueryError::database(format!("Failed to set SQL: {:?}", e)))?;
    if !params.is_empty() {
        Reflect::set(
            &item,
            &JsValue::from_str("params"),
            &sql_params_to_js(params),
        )
        .map_err(|e| LocalDbQueryError::database(format!("Failed to set params: {:?}", e)))?;
    }
    Ok(item)
}

fn method(local_db: &JsValue, name: &str) -> Result<js_sys::Function, LocalDbQueryError> {
    Reflect::get(local_db, &JsValue::from_str(name))
        .map_err(|e| LocalDbQueryError::database(format!("Failed to read localDb.{name}: {e:?}")))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| LocalDbQueryError::database(format!("localDb.{name} must be a function")))
}

fn sql_params_to_js(params: &[SqlValue]) -> JsValue {
    let array = Array::new();
    params.iter().for_each(|param| {
        let js_param = match param {
            SqlValue::Text(text) => JsValue::from_str(text),
            SqlValue::I64(value) => JsValue::from(BigInt::from(*value)),
            SqlValue::U64(value) => JsValue::from(BigInt::from(*value)),
            SqlValue::Null => JsValue::NULL,
        };
        array.push(&js_param);
    });
    JsValue::from(array)
}

impl JsCallbackExecutor {
    async fn import_dumps(&self, dumps: Vec<Arc<String>>) -> Result<(), LocalDbQueryError> {
        if dumps.is_empty() {
            return Ok(());
        }
        let serialize = Arc::clone(&self.serialize).lock_owned().await;
        // The producer wraps its data-only dump in BEGIN/COMMIT. Keep index
        // changes inside that same worker-owned import transaction.
        let data_sql = dumps
            .iter()
            .map(|dump| {
                dump.trim()
                    .strip_prefix("BEGIN;")
                    .and_then(|sql| sql.strip_suffix("COMMIT;"))
                    .ok_or_else(|| {
                        LocalDbQueryError::database("Unsupported SQL dump transaction markers")
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Both cohort and per-network imports hold the same cross-tab lock
        // before reading watermarks. A competing import cannot populate the DB
        // between this preflight and opening our worker session.
        let started = js_sys::Date::now();
        let ownership = loop {
            match super::pipeline::runner::leadership::acquire_import().await {
                Ok(Some(guard)) => break Some(guard),
                Ok(None) if within_import_wait(started) => {
                    gloo_timers::future::TimeoutFuture::new(IMPORT_WAIT_POLL_MS).await;
                }
                Ok(None) => {
                    return Err(LocalDbQueryError::database(
                        "Timed out waiting for import ownership",
                    ));
                }
                Err(error) => {
                    // Import still works through the SDK lease without Web
                    // Locks, but preflight cannot safely justify index drops.
                    tracing::warn!(%error, "Import ownership unavailable; retaining indexes");
                    break None;
                }
            }
        };
        // Only a globally fresh DB benefits from rebuilding every index. Adding
        // seeds to a populated DB keeps indexes over its existing history intact.
        let watermarks = self
            .invoke_read_unlocked(&SqlStatement::new(
                "SELECT 1 AS present FROM target_watermarks LIMIT 1",
            ))
            .await?;
        let has_watermarks: Vec<serde_json::Value> =
            serde_json::from_str(&watermarks).map_err(|_| LocalDbQueryError::invalid_response())?;
        let defer_indexes = ownership.is_some() && has_watermarks.is_empty();
        let mut prefix = String::from("BEGIN;\n");
        let mut suffix = String::new();
        if defer_indexes {
            let index_rows = self
                .invoke_read_unlocked(&SqlStatement::new(
                    "SELECT m.name, m.sql FROM sqlite_master AS m JOIN pragma_index_list(m.tbl_name) AS i ON i.name = m.name WHERE m.type = 'index' AND m.sql IS NOT NULL AND i.\"unique\" = 0 ORDER BY m.name",
                ))
                .await?;
            let index_rows: Vec<IndexDefinition> = serde_json::from_str(&index_rows)
                .map_err(|_| LocalDbQueryError::invalid_response())?;
            for row in &index_rows {
                prefix.push_str(&format!(
                    "DROP INDEX \"{}\";\n",
                    row.name.replace('"', "\"\"")
                ));
                suffix.push_str(&row.sql);
                suffix.push_str(";\n");
            }
        }
        suffix.push_str("COMMIT;\n");

        let mut guard = ImportSessionGuard {
            executor: self.clone(),
            pending: None,
            serialize: Some(serialize),
            ownership,
        };
        let started = js_sys::Date::now();
        let id = loop {
            let begin = self.import_promise(&self.begin_import_callback, ImportArgs::Begin)?;
            guard.pending = Some(PendingImport::Beginning(begin.clone()));
            let result = JsFuture::from(begin)
                .await
                .map_err(|e| LocalDbQueryError::database(format!("Import promise failed: {e:?}")))
                .and_then(decode_string_result);
            match result {
                Err(err) if err.is_import_in_progress() && within_import_wait(started) => {
                    guard.pending = None;
                    // An older SDK caller may not participate in our Web Lock.
                    // Its import invalidates preflight; retain indexes rather
                    // than rebuild over data committed while we waited.
                    prefix = String::from("BEGIN;\n");
                    suffix = String::from("COMMIT;\n");
                    gloo_timers::future::TimeoutFuture::new(IMPORT_WAIT_POLL_MS).await;
                }
                result => break result?,
            }
        };
        guard.pending = Some(PendingImport::Active {
            id: id.clone(),
            in_flight: None,
        });
        let import_result = async {
            self.append_import_sql(&mut guard, &id, &prefix).await?;
            for sql in data_sql {
                self.append_import_sql(&mut guard, &id, sql).await?;
                // Keep a trailing line comment in one dump from consuming the
                // first statement or index definitions in the next dump.
                self.append_import_sql(&mut guard, &id, "\n").await?;
            }
            self.append_import_sql(&mut guard, &id, &suffix).await?;
            guard
                .invoke(&self.finish_import_callback, ImportArgs::Session(&id))
                .await?;
            Ok(())
        }
        .await;
        if import_result.is_err() {
            if guard
                .invoke(&self.cancel_import_callback, ImportArgs::Session(&id))
                .await
                .is_ok()
            {
                guard.disarm();
            }
        } else {
            guard.disarm();
        }
        import_result
    }
}

// SAFETY: WASM builds run on a single thread; the wrapped JavaScript callback is only invoked on
// that thread, so sharing the executor across async tasks is safe.
unsafe impl Sync for JsCallbackExecutor {}

#[cfg(target_family = "wasm")]
#[async_trait(?Send)]
impl LocalDbQueryExecutor for JsCallbackExecutor {
    async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
        let _guard = self.serialize.lock().await;
        if !batch.is_transaction() {
            return Err(LocalDbQueryError::database(
                "SQL statement batch must be wrapped in a transaction",
            ));
        }

        self.invoke_transaction_unlocked(batch).await
    }

    async fn execute_sql_dump(&self, dump: Arc<String>) -> Result<(), LocalDbQueryError> {
        self.import_dumps(vec![dump]).await
    }

    async fn execute_sql_dumps(&self, dumps: Vec<Arc<String>>) -> Result<(), LocalDbQueryError> {
        self.import_dumps(dumps).await
    }

    async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
        self.invoke_statement(stmt).await
    }

    async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
    where
        T: FromDbJson,
    {
        let value = self.invoke_statement(stmt).await?;
        serde_json::from_str(&value)
            .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
    }

    async fn query_json_retryable<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
    where
        T: FromDbJson,
    {
        let value = self.invoke_read(stmt).await?;
        serde_json::from_str(&value)
            .map_err(|err| LocalDbQueryError::deserialization(err.to_string()))
    }

    async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
        let _guard = self.serialize.lock().await;
        let result = self.wipe_callback.call0(&self.local_db).map_err(|e| {
            LocalDbQueryError::database(format!(
                "JavaScript wipe callback invocation failed: {:?}",
                e
            ))
        })?;

        let promise = js_sys::Promise::resolve(&result);
        let future = JsFuture::from(promise);
        let js_result = future.await.map_err(|e| {
            LocalDbQueryError::database(format!("Wipe promise resolution failed: {:?}", e))
        })?;

        let wasm_result: WasmEncodedResult<()> = serde_wasm_bindgen::from_value(js_result)
            .map_err(|_| LocalDbQueryError::invalid_response())?;

        match wasm_result {
            WasmEncodedResult::Success { .. } => Ok(()),
            WasmEncodedResult::Err { error, .. } => {
                Err(LocalDbQueryError::database(error.readable_msg))
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    #[cfg(target_family = "wasm")]
    use super::*;

    #[cfg(target_family = "wasm")]
    pub use wasm_tests::create_sql_capturing_callback;

    #[cfg(target_family = "wasm")]
    mod wasm_tests {
        use super::*;
        use js_sys::Function;
        use serde::{Deserialize, Serialize};
        use wasm_bindgen::JsCast;
        use wasm_bindgen_test::*;
        use wasm_bindgen_utils::prelude::serde_wasm_bindgen;
        use wasm_bindgen_utils::prelude::JsValue;
        use wasm_bindgen_utils::result::{WasmEncodedError, WasmEncodedResult};

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct TestData {
            id: u32,
            name: String,
        }

        pub fn create_success_callback(response: &str) -> Function {
            let success = WasmEncodedResult::Success::<String> {
                value: response.to_string(),
                error: None,
            };
            let js_value = serde_wasm_bindgen::to_value(&success).unwrap();
            Function::new_no_args(&format!(
                "return {}",
                js_sys::JSON::stringify(&js_value)
                    .unwrap()
                    .as_string()
                    .unwrap()
            ))
        }

        pub fn create_sql_capturing_callback(
            response: &str,
            store: std::rc::Rc<std::cell::RefCell<(String, JsValue)>>,
        ) -> Function {
            use wasm_bindgen::prelude::Closure;

            let response = response.to_string();
            let store_clone = store.clone();
            let closure = Closure::wrap(Box::new(
                move |sql: String, params: JsValue| -> wasm_bindgen::JsValue {
                    *store_clone.borrow_mut() = (sql, params);
                    let result = WasmEncodedResult::Success::<String> {
                        value: response.clone(),
                        error: None,
                    };
                    serde_wasm_bindgen::to_value(&result).unwrap()
                },
            )
                as Box<dyn FnMut(String, JsValue) -> wasm_bindgen::JsValue>);

            let func: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();
            func
        }

        fn create_local_db(
            query: Option<Function>,
            wipe: Option<Function>,
            transaction: Option<Function>,
        ) -> JsValue {
            let local_db = js_sys::Object::new();
            if let Some(query) = query {
                Reflect::set(&local_db, &JsValue::from_str("query"), &query).unwrap();
            }
            if let Some(wipe) = wipe {
                Reflect::set(&local_db, &JsValue::from_str("wipeAndRecreate"), &wipe).unwrap();
            }
            if let Some(transaction) = transaction {
                Reflect::set(&local_db, &JsValue::from_str("transaction"), &transaction).unwrap();
            }
            for name in [
                "beginSqlDumpImport",
                "appendSqlDumpChunk",
                "finishSqlDumpImport",
                "cancelSqlDumpImport",
            ] {
                Reflect::set(
                    &local_db,
                    &JsValue::from_str(name),
                    &Function::new_no_args("return { value: '', error: null };"),
                )
                .unwrap();
            }
            local_db.into()
        }

        fn success_wipe_callback() -> Function {
            Function::new_no_args("return { value: undefined, error: null };")
        }

        fn success_transaction_callback() -> Function {
            Function::new_no_args("return { value: '', error: null };")
        }

        #[wasm_bindgen_test]
        async fn test_query_json_success_case() {
            let test_data = vec![
                TestData {
                    id: 1,
                    name: "Alice".to_string(),
                },
                TestData {
                    id: 2,
                    name: "Bob".to_string(),
                },
            ];
            let json_data = serde_json::to_string(&test_data).unwrap();
            let callback = create_success_callback(&json_data);
            let exec = JsCallbackExecutor::from_ref(&callback);

            let result: Result<Vec<TestData>, LocalDbQueryError> = exec
                .query_json(&SqlStatement::new("SELECT * FROM users"))
                .await;
            assert!(result.is_ok());
            let data = result.unwrap();
            assert_eq!(data.len(), 2);
            assert_eq!(data[0].name, "Alice");
            assert_eq!(data[1].name, "Bob");
        }

        #[wasm_bindgen_test]
        async fn test_query_text_success() {
            let callback = create_success_callback("text-result");
            let exec = JsCallbackExecutor::from_ref(&callback);
            let val = exec
                .query_text(&SqlStatement::new("SELECT 1"))
                .await
                .unwrap();
            assert_eq!(val, "text-result");
        }

        #[wasm_bindgen_test]
        async fn passes_undefined_params_when_empty() {
            use std::cell::RefCell;
            use std::rc::Rc;
            use wasm_bindgen::JsValue;

            let store = Rc::new(RefCell::new((String::new(), JsValue::UNDEFINED)));
            let callback = create_sql_capturing_callback("OK", store.clone());
            let exec = JsCallbackExecutor::from_ref(&callback);

            let _ = exec
                .query_text(&SqlStatement::new("SELECT 42"))
                .await
                .unwrap();

            let (_, captured_params) = store.borrow().clone();
            assert!(captured_params.is_undefined());
        }

        #[wasm_bindgen_test]
        async fn passes_array_params_when_non_empty() {
            use js_sys::Array;
            use std::cell::RefCell;
            use std::rc::Rc;
            use wasm_bindgen::JsValue;

            let store = Rc::new(RefCell::new((String::new(), JsValue::UNDEFINED)));
            let callback = create_sql_capturing_callback("OK", store.clone());
            let exec = JsCallbackExecutor::from_ref(&callback);

            // Build a statement with parameters
            let mut stmt = SqlStatement::new("SELECT ?1, ?2");
            let _ = stmt.push(123i64);
            let _ = stmt.push("abc");

            let _ = exec.query_text(&stmt).await.unwrap();

            let (_, captured_params) = store.borrow().clone();

            // Ensure non-empty params are passed as a JavaScript Array
            assert!(Array::is_array(&captured_params));

            // Decode and assert expected contents and length
            let decoded = Array::from(&captured_params);
            assert_eq!(decoded.length(), 2);

            let first = decoded.get(0).dyn_into::<BigInt>().unwrap();
            assert_eq!(first.to_string(10).unwrap().as_string().unwrap(), "123");

            let second = decoded.get(1);
            assert_eq!(
                second.as_string().unwrap(),
                "abc",
                "expected text param in position 2"
            );
        }

        #[wasm_bindgen_test]
        async fn execute_batch_uses_transaction_callback_with_inner_statements() {
            use std::cell::RefCell;
            use std::rc::Rc;
            use wasm_bindgen::prelude::Closure;

            let calls: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
            let calls_clone = calls.clone();
            let closure = Closure::wrap(Box::new(move |statements: JsValue| -> JsValue {
                calls_clone.borrow_mut().push(statements);
                let result = WasmEncodedResult::Success::<String> {
                    value: String::new(),
                    error: None,
                };
                serde_wasm_bindgen::to_value(&result).unwrap()
            }) as Box<dyn FnMut(JsValue) -> JsValue>);

            let callback: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();

            let exec = JsCallbackExecutor::new(create_local_db(
                Some(create_success_callback("[]")),
                Some(success_wipe_callback()),
                Some(callback),
            ))
            .unwrap();

            let mut batch = SqlStatementBatch::new();
            batch.add(SqlStatement::new("CREATE TABLE example (val INTEGER)"));
            let mut insert = SqlStatement::new("INSERT INTO example (val) VALUES (?1)");
            insert.push(42i64);
            batch.add(insert);
            batch.add(SqlStatement::new("DELETE FROM example WHERE val = 0"));

            let batch = batch.ensure_transaction();

            exec.execute_batch(&batch).await.unwrap();

            let calls = calls.borrow();
            assert_eq!(calls.len(), 1);
            let statements = Array::from(&calls[0]);
            assert_eq!(statements.length(), 3);

            let first = js_sys::Object::from(statements.get(0));
            assert_eq!(
                Reflect::get(&first, &JsValue::from_str("sql"))
                    .unwrap()
                    .as_string()
                    .unwrap(),
                "CREATE TABLE example (val INTEGER)"
            );
            assert!(Reflect::get(&first, &JsValue::from_str("params"))
                .unwrap()
                .is_undefined());

            let second = js_sys::Object::from(statements.get(1));
            assert_eq!(
                Reflect::get(&second, &JsValue::from_str("sql"))
                    .unwrap()
                    .as_string()
                    .unwrap(),
                "INSERT INTO example (val) VALUES (?1)"
            );
            let params_value = Reflect::get(&second, &JsValue::from_str("params")).unwrap();
            assert!(Array::is_array(&params_value));
            let decoded = Array::from(&params_value);
            assert_eq!(decoded.length(), 1);
            let first = decoded.get(0).dyn_into::<BigInt>().unwrap();
            assert_eq!(first.to_string(10).unwrap().as_string().unwrap(), "42");

            let third = js_sys::Object::from(statements.get(2));
            assert_eq!(
                Reflect::get(&third, &JsValue::from_str("sql"))
                    .unwrap()
                    .as_string()
                    .unwrap(),
                "DELETE FROM example WHERE val = 0"
            );
            assert!(Reflect::get(&third, &JsValue::from_str("params"))
                .unwrap()
                .is_undefined());
        }

        #[wasm_bindgen_test]
        async fn execute_batch_propagates_transaction_error() {
            use std::cell::RefCell;
            use std::rc::Rc;
            use wasm_bindgen::prelude::Closure;

            let calls: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
            let calls_clone = calls.clone();
            let closure = Closure::wrap(Box::new(move |_statements: JsValue| -> JsValue {
                *calls_clone.borrow_mut() += 1;
                let result = WasmEncodedResult::Err::<String> {
                    value: None,
                    error: WasmEncodedError {
                        msg: "boom".to_string(),
                        readable_msg: "boom readable".to_string(),
                    },
                };
                serde_wasm_bindgen::to_value(&result).unwrap()
            }) as Box<dyn FnMut(JsValue) -> JsValue>);
            let callback: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();

            let exec = JsCallbackExecutor::new(create_local_db(
                Some(create_success_callback("[]")),
                Some(success_wipe_callback()),
                Some(callback),
            ))
            .unwrap();

            let mut batch = SqlStatementBatch::new();
            batch.add(SqlStatement::new("INSERT INTO rollback_test VALUES (1)"));
            let batch = batch.ensure_transaction();

            let err = exec.execute_batch(&batch).await.unwrap_err();
            assert!(matches!(err, LocalDbQueryError::Database { .. }));
            assert!(err.to_string().contains("boom readable"));
            assert_eq!(*calls.borrow(), 1);
        }

        #[wasm_bindgen_test]
        async fn query_waits_for_another_tabs_import() {
            use std::cell::RefCell;
            use std::rc::Rc;
            use wasm_bindgen::prelude::Closure;

            let calls: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
            let calls_clone = calls.clone();
            let closure =
                Closure::wrap(Box::new(move |_sql: String, _params: JsValue| -> JsValue {
                    *calls_clone.borrow_mut() += 1;
                    if *calls_clone.borrow() < 3 {
                        let busy = "JavaScript error: JsValue(\"SQL dump import is in progress.\")";
                        let result = WasmEncodedResult::Err::<String> {
                            value: None,
                            error: WasmEncodedError {
                                msg: busy.to_string(),
                                readable_msg: busy.to_string(),
                            },
                        };
                        return serde_wasm_bindgen::to_value(&result).unwrap();
                    }
                    let result = WasmEncodedResult::Success::<String> {
                        value: "ok".to_string(),
                        error: None,
                    };
                    serde_wasm_bindgen::to_value(&result).unwrap()
                })
                    as Box<dyn FnMut(String, JsValue) -> JsValue>);
            let callback: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();
            let exec = JsCallbackExecutor::from_ref(&callback);

            let value = exec
                .query_text(&SqlStatement::new("PRAGMA quick_check"))
                .await
                .unwrap();

            assert_eq!(value, "ok");
            assert_eq!(*calls.borrow(), 3);
        }

        #[wasm_bindgen_test]
        async fn explicitly_retryable_queries_retry_follower_timeouts() {
            let callback = Function::new_with_args(
                "sql",
                "this.calls = (this.calls || 0) + 1; return this.calls === 1 ? { value: undefined, error: { msg: 'timeout', readableMsg: 'JavaScript error: JsValue(\"Query timeout\")' } } : { value: '[]', error: null };",
            );
            // Exercise the erased LocalDb wrapper as well as the executor.
            let exec = LocalDb::new(JsCallbackExecutor::from_ref(&callback));
            let rows: Vec<serde_json::Value> = exec
                .query_json_retryable(&SqlStatement::new("PRAGMA quick_check"))
                .await
                .unwrap();
            assert!(rows.is_empty());

            let callback = Function::new_with_args(
                "sql",
                "return { value: undefined, error: { msg: 'timeout', readableMsg: 'JavaScript error: JsValue(\"Query timeout\")' } };",
            );
            let exec = JsCallbackExecutor::from_ref(&callback);
            let err = exec
                .query_text(&SqlStatement::new("INSERT INTO t VALUES (1)"))
                .await
                .unwrap_err();
            assert!(err.is_worker_unavailable());
        }

        #[wasm_bindgen_test]
        async fn generic_json_query_never_replays_timed_out_writes() {
            for sql in [
                "INSERT INTO t VALUES (1) RETURNING v",
                "WITH input(v) AS (SELECT 1) INSERT INTO t SELECT v FROM input RETURNING v",
                "SELECT 1; INSERT INTO t VALUES (1) RETURNING v;",
            ] {
                let local_db = create_local_db(
                    Some(Function::new_with_args(
                        "sql",
                        "this.writes = (this.writes || 0) + 1; return { value: undefined, error: { msg: 'timeout', readableMsg: 'Query timeout' } };",
                    )),
                    Some(success_wipe_callback()),
                    Some(success_transaction_callback()),
                );
                let exec = LocalDb::new(JsCallbackExecutor::new(local_db.clone()).unwrap());

                let err = exec
                    .query_json::<Vec<serde_json::Value>>(&SqlStatement::new(sql))
                    .await
                    .unwrap_err();
                assert!(err.is_worker_unavailable());
                assert_eq!(
                    Reflect::get(&local_db, &JsValue::from_str("writes"))
                        .unwrap()
                        .as_f64(),
                    Some(1.0)
                );
            }
        }

        #[wasm_bindgen_test]
        async fn execute_batch_rejects_non_transactions() {
            let callback = create_success_callback("");
            let exec = JsCallbackExecutor::from_ref(&callback);
            let batch = SqlStatementBatch::from(vec![SqlStatement::new("SELECT 1")]);

            let err = exec.execute_batch(&batch).await.unwrap_err();
            assert!(matches!(err, LocalDbQueryError::Database { .. }));
        }

        #[wasm_bindgen_test]
        async fn test_callback_throws() {
            // callback that throws synchronously
            let callback = Function::new_with_args("sql, params", "throw new Error('boom')");
            let exec = JsCallbackExecutor::from_ref(&callback);
            let err = exec
                .query_text(&SqlStatement::new("SELECT 1"))
                .await
                .err()
                .unwrap();
            match err {
                LocalDbQueryError::Database { .. } => {}
                other => panic!("unexpected error variant: {:?}", other),
            }
        }

        #[wasm_bindgen_test]
        async fn test_promise_rejects() {
            // callback returns a rejected Promise
            let callback =
                Function::new_with_args("sql, params", "return Promise.reject('rejected')");
            let exec = JsCallbackExecutor::from_ref(&callback);
            let err = exec
                .query_text(&SqlStatement::new("SELECT 1"))
                .await
                .err()
                .unwrap();
            match err {
                LocalDbQueryError::Database { .. } => {}
                other => panic!("unexpected error variant: {:?}", other),
            }
        }

        #[wasm_bindgen_test]
        async fn test_invalid_wrapper_yields_invalid_response() {
            // returns a plain string instead of WasmEncodedResult
            let callback = Function::new_with_args("sql, params", "return 'not-a-wrapper'");
            let exec = JsCallbackExecutor::from_ref(&callback);
            let res: Result<Vec<TestData>, LocalDbQueryError> =
                exec.query_json(&SqlStatement::new("SELECT 1")).await;
            assert!(matches!(res, Err(LocalDbQueryError::InvalidResponse)));
        }

        #[wasm_bindgen_test]
        async fn test_deserialization_error() {
            // Success wrapper but invalid JSON payload
            use wasm_bindgen_utils::result::WasmEncodedResult;
            let store =
                std::rc::Rc::new(std::cell::RefCell::new((String::new(), JsValue::UNDEFINED)));
            let store_clone = store.clone();
            let closure = wasm_bindgen::prelude::Closure::wrap(Box::new(
                move |sql: String, params: JsValue| -> JsValue {
                    *store_clone.borrow_mut() = (sql, params);
                    let result: WasmEncodedResult<String> = WasmEncodedResult::Success {
                        value: "not-json".to_string(),
                        error: None,
                    };
                    serde_wasm_bindgen::to_value(&result).unwrap()
                },
            )
                as Box<dyn FnMut(String, JsValue) -> JsValue>);
            let callback: Function = closure.as_ref().clone().unchecked_into();
            closure.forget();

            let exec = JsCallbackExecutor::from_ref(&callback);
            let res: Result<Vec<TestData>, LocalDbQueryError> =
                exec.query_json(&SqlStatement::new("SELECT 1")).await;
            assert!(matches!(
                res,
                Err(LocalDbQueryError::Deserialization { .. })
            ));
        }

        #[wasm_bindgen_test]
        async fn constructor_requires_wipe_callback() {
            let callback = create_success_callback("[]");
            let result = JsCallbackExecutor::new(create_local_db(
                Some(callback),
                None,
                Some(success_transaction_callback()),
            ));

            let Err(err) = result else {
                panic!("constructor should reject missing wipe callback");
            };
            assert!(matches!(err, LocalDbQueryError::Database { .. }));
            assert!(err
                .to_string()
                .contains("localDb.wipeAndRecreate must be a function"));
        }

        #[wasm_bindgen_test]
        async fn wipe_and_recreate_calls_wipe_callback() {
            use std::cell::RefCell;
            use std::rc::Rc;
            use wasm_bindgen::prelude::Closure;

            let wipe_called = Rc::new(RefCell::new(false));
            let wipe_called_clone = wipe_called.clone();
            let wipe_closure = Closure::wrap(Box::new(move || -> JsValue {
                *wipe_called_clone.borrow_mut() = true;
                let result = WasmEncodedResult::Success::<()> {
                    value: (),
                    error: None,
                };
                serde_wasm_bindgen::to_value(&result).unwrap()
            }) as Box<dyn FnMut() -> JsValue>);
            let wipe_callback: Function = wipe_closure.as_ref().clone().unchecked_into();
            wipe_closure.forget();

            let callback = create_success_callback("[]");
            let exec = JsCallbackExecutor::new(create_local_db(
                Some(callback),
                Some(wipe_callback),
                Some(success_transaction_callback()),
            ))
            .unwrap();

            exec.wipe_and_recreate().await.unwrap();

            assert!(
                *wipe_called.borrow(),
                "wipe callback should have been called"
            );
        }

        #[wasm_bindgen_test]
        async fn wipe_and_recreate_propagates_callback_error() {
            let wipe_callback = Function::new_no_args(&{
                let error = WasmEncodedResult::Err::<()> {
                    value: None,
                    error: WasmEncodedError {
                        msg: "wipe failed".to_string(),
                        readable_msg: "wipe failed readable".to_string(),
                    },
                };
                let js_value = serde_wasm_bindgen::to_value(&error).unwrap();
                format!(
                    "return {}",
                    js_sys::JSON::stringify(&js_value)
                        .unwrap()
                        .as_string()
                        .unwrap()
                )
            });

            let callback = create_success_callback("[]");
            let exec = JsCallbackExecutor::new(create_local_db(
                Some(callback),
                Some(wipe_callback),
                Some(success_transaction_callback()),
            ))
            .unwrap();

            let result = exec.wipe_and_recreate().await;
            assert!(matches!(result, Err(LocalDbQueryError::Database { .. })));
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("wipe failed readable"));
        }

        #[wasm_bindgen_test]
        async fn sql_dump_import_keeps_indexes_and_data_in_one_bounded_session() {
            use std::cell::RefCell;
            use wasm_bindgen::prelude::Closure;

            let chunks = Rc::new(RefCell::new(Vec::<String>::new()));
            let captured = Rc::clone(&chunks);
            let append = Closure::wrap(Box::new(move |id: String, sql: String| -> JsValue {
                assert_eq!(id, "session");
                assert!(sql.len() <= 256 * 1024);
                captured.borrow_mut().push(sql);
                serde_wasm_bindgen::to_value(&WasmEncodedResult::success(String::new())).unwrap()
            }) as Box<dyn FnMut(String, String) -> JsValue>);
            let local_db = create_local_db(
                Some(Function::new_with_args(
                    "sql",
                    "return sql.includes('target_watermarks') ? { value: '[]', error: null } : { value: '[{\"name\":\"idx_t_value\",\"sql\":\"CREATE INDEX idx_t_value ON t(value)\"}]', error: null };",
                )),
                Some(success_wipe_callback()),
                Some(success_transaction_callback()),
            );
            Reflect::set(
                &local_db,
                &JsValue::from_str("beginSqlDumpImport"),
                &Function::new_no_args("return { value: 'session', error: null };"),
            )
            .unwrap();
            Reflect::set(
                &local_db,
                &JsValue::from_str("appendSqlDumpChunk"),
                append.as_ref(),
            )
            .unwrap();
            append.forget();
            let exec = JsCallbackExecutor::new(local_db).unwrap();
            let inserts = "INSERT INTO t VALUES ('😀');\n".repeat(30_000);
            let dump = Arc::new(format!("BEGIN;\n{inserts}COMMIT;\n"));
            exec.execute_sql_dump(dump).await.unwrap();

            let chunks = chunks.borrow();
            assert!(chunks.len() > 3);
            let sql = chunks.concat();
            assert!(sql.starts_with("BEGIN;\nDROP INDEX \"idx_t_value\";"));
            assert!(sql.contains(&inserts));
            assert!(sql.ends_with("CREATE INDEX idx_t_value ON t(value);\nCOMMIT;\n"));
            assert_eq!(sql.matches("BEGIN;").count(), 1);
            assert_eq!(sql.matches("COMMIT;").count(), 1);
        }

        #[wasm_bindgen_test]
        async fn later_dump_keeps_existing_indexes() {
            use std::cell::RefCell;
            use wasm_bindgen::prelude::Closure;

            let chunks = Rc::new(RefCell::new(Vec::<String>::new()));
            let captured = Rc::clone(&chunks);
            let append = Closure::wrap(Box::new(move |_id: String, sql: String| -> JsValue {
                captured.borrow_mut().push(sql);
                serde_wasm_bindgen::to_value(&WasmEncodedResult::success(String::new())).unwrap()
            }) as Box<dyn FnMut(String, String) -> JsValue>);
            let local_db = create_local_db(
                Some(Function::new_with_args(
                    "sql",
                    "if (!sql.includes('target_watermarks')) throw new Error('unexpected index query'); return { value: '[{\"present\":1}]', error: null };",
                )),
                Some(success_wipe_callback()),
                Some(success_transaction_callback()),
            );
            Reflect::set(
                &local_db,
                &JsValue::from_str("beginSqlDumpImport"),
                &Function::new_no_args("return { value: 'session', error: null };"),
            )
            .unwrap();
            Reflect::set(
                &local_db,
                &JsValue::from_str("appendSqlDumpChunk"),
                append.as_ref(),
            )
            .unwrap();
            append.forget();

            let exec = JsCallbackExecutor::new(local_db).unwrap();
            exec.execute_sql_dump(Arc::new(
                "BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string(),
            ))
            .await
            .unwrap();

            let sql = chunks.borrow().concat();
            assert_eq!(sql.matches("BEGIN;").count(), 1);
            assert_eq!(sql.matches("COMMIT;").count(), 1);
            assert!(!sql.contains("DROP INDEX"));
            assert!(!sql.contains("CREATE INDEX"));
            assert!(sql.contains("INSERT INTO t VALUES (1);"));
        }

        #[wasm_bindgen_test]
        async fn cohort_rebuilds_only_on_a_fresh_database() {
            for populated in [false, true] {
                let local_db = Function::new_with_args("populated",
                r#"return {
                chunks: [], begins: 0, finishes: 0,
                query(sql) {
                    return {value: sql.includes('target_watermarks') ? (populated ? '[{"present":1}]' : '[]') :
                        '[{"name":"idx_t_value","sql":"CREATE INDEX idx_t_value ON t(value)"}]'};
                },
                transaction() { throw new Error('not a statement transaction'); },
                wipeAndRecreate() { throw new Error('must not wipe'); },
                beginSqlDumpImport() { this.begins++; return {value:'session'}; },
                appendSqlDumpChunk(id, sql) { this.chunks.push(sql); return {value:''}; },
                finishSqlDumpImport() { this.finishes++; return {value:''}; },
                cancelSqlDumpImport() { throw new Error('must not cancel'); }
            };"#,
            )
            .call1(&JsValue::UNDEFINED, &JsValue::from_bool(populated))
            .unwrap();
                let db = LocalDb::new(JsCallbackExecutor::new(local_db.clone()).unwrap());
                db.execute_sql_dumps(vec![
                    Arc::new(
                        "BEGIN; INSERT INTO t VALUES (1); -- first network\nCOMMIT;".to_string(),
                    ),
                    Arc::new("BEGIN; INSERT INTO t VALUES (2); COMMIT;".to_string()),
                ])
                .await
                .unwrap();
                let chunks = Reflect::get(&local_db, &JsValue::from_str("chunks"))
                    .unwrap()
                    .dyn_into::<Array>()
                    .unwrap();
                let sql = chunks
                    .iter()
                    .map(|v| v.as_string().unwrap())
                    .collect::<String>();
                assert_eq!(sql.matches("BEGIN;").count(), 1);
                assert_eq!(sql.matches("COMMIT;").count(), 1);
                assert_eq!(sql.matches("DROP INDEX").count(), usize::from(!populated));
                assert_eq!(sql.matches("CREATE INDEX").count(), usize::from(!populated));
                if !populated {
                    assert!(sql.find("VALUES (2)").unwrap() < sql.find("CREATE INDEX").unwrap());
                }
                assert_eq!(
                    Reflect::get(&local_db, &JsValue::from_str("begins"))
                        .unwrap()
                        .as_f64(),
                    Some(1.0)
                );
                assert_eq!(
                    Reflect::get(&local_db, &JsValue::from_str("finishes"))
                        .unwrap()
                        .as_f64(),
                    Some(1.0)
                );
            }
        }

        #[wasm_bindgen_test]
        async fn import_lease_contention_retains_preflight_indexes() {
            let local_db = Function::new_no_args(r#"return {
                chunks: [], begins: 0,
                query(sql) { return {value: sql.includes('target_watermarks') ? '[]' :
                    '[{"name":"idx_t_value","sql":"CREATE INDEX idx_t_value ON t(value)"}]'}; },
                transaction() {}, wipeAndRecreate() {},
                beginSqlDumpImport() {
                    if (++this.begins === 1) return {error:{msg:'busy',readableMsg:'SQL dump import is in progress'}};
                    return {value:'session'};
                },
                appendSqlDumpChunk(id, sql) { this.chunks.push(sql); return {value:''}; },
                finishSqlDumpImport() { return {value:''}; }, cancelSqlDumpImport() {}
            };"#).call0(&JsValue::UNDEFINED).unwrap();
            let exec = JsCallbackExecutor::new(local_db.clone()).unwrap();
            exec.execute_sql_dump(Arc::new("BEGIN; INSERT INTO t VALUES (1); COMMIT;".into()))
                .await
                .unwrap();
            let chunks = Reflect::get(&local_db, &"chunks".into())
                .unwrap()
                .dyn_into::<Array>()
                .unwrap();
            let sql = chunks
                .iter()
                .map(|v| v.as_string().unwrap())
                .collect::<String>();
            assert!(sql.contains("INSERT INTO t VALUES (1)"));
            assert!(!sql.contains("DROP INDEX"));
            assert!(!sql.contains("CREATE INDEX"));
            assert_eq!(
                Reflect::get(&local_db, &"begins".into()).unwrap().as_f64(),
                Some(2.0)
            );
        }

        #[wasm_bindgen_test]
        async fn cohort_rejects_bad_envelopes_before_opening_a_session() {
            let local_db = Function::new_no_args(
                r#"return {
                query() { throw new Error('must validate before DB access'); },
                transaction() {}, wipeAndRecreate() {},
                beginSqlDumpImport() { throw new Error('must not begin'); },
                appendSqlDumpChunk() {}, finishSqlDumpImport() {}, cancelSqlDumpImport() {}
            };"#,
            )
            .call0(&JsValue::UNDEFINED)
            .unwrap();
            let exec = JsCallbackExecutor::new(local_db).unwrap();
            let error = exec
                .execute_sql_dumps(vec![
                    Arc::new("BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string()),
                    Arc::new("INSERT INTO t VALUES (2);".to_string()),
                ])
                .await
                .unwrap_err();
            assert!(error
                .to_string()
                .contains("Unsupported SQL dump transaction markers"));
        }

        #[wasm_bindgen_test]
        async fn second_dump_failure_cancels_the_entire_cohort() {
            let local_db = Function::new_no_args(r#"return {
                cancelled: 0, finished: 0,
                query(sql) { return {value:'[]'}; }, transaction() {}, wipeAndRecreate() {},
                beginSqlDumpImport() { return {value:'session'}; },
                appendSqlDumpChunk(id, sql) {
                    if(sql.includes('bad_second_dump')) return {error:{msg:'bad SQL',readableMsg:'bad SQL'}};
                    return {value:''};
                },
                finishSqlDumpImport() { this.finished++; return {value:''}; },
                cancelSqlDumpImport() { this.cancelled++; return {value:''}; }
            };"#).call0(&JsValue::UNDEFINED).unwrap();
            let exec = JsCallbackExecutor::new(local_db.clone()).unwrap();
            assert!(exec
                .execute_sql_dumps(vec![
                    Arc::new("BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string()),
                    Arc::new("BEGIN; bad_second_dump; COMMIT;".to_string()),
                ])
                .await
                .is_err());
            assert_eq!(
                Reflect::get(&local_db, &JsValue::from_str("cancelled"))
                    .unwrap()
                    .as_f64(),
                Some(1.0)
            );
            assert_eq!(
                Reflect::get(&local_db, &JsValue::from_str("finished"))
                    .unwrap()
                    .as_f64(),
                Some(0.0)
            );
        }

        #[wasm_bindgen_test]
        async fn sql_dump_import_waits_for_another_tabs_import_to_begin() {
            let local_db = create_local_db(
                Some(Function::new_with_args(
                    "sql",
                    "return { value: '[{\"present\":1}]', error: null };",
                )),
                Some(success_wipe_callback()),
                Some(success_transaction_callback()),
            );
            Reflect::set(
                &local_db,
                &JsValue::from_str("beginSqlDumpImport"),
                &Function::new_no_args(
                    "this.begins = (this.begins || 0) + 1; return this.begins < 3 ? { value: undefined, error: { msg: 'busy', readableMsg: 'JavaScript error: JsValue(\"SQL dump import is in progress.\")' } } : { value: 'session', error: null };",
                ),
            )
            .unwrap();
            let exec = JsCallbackExecutor::new(local_db.clone()).unwrap();

            exec.execute_sql_dump(Arc::new(
                "BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string(),
            ))
            .await
            .unwrap();

            let begins = Reflect::get(&local_db, &JsValue::from_str("begins")).unwrap();
            assert_eq!(begins.as_f64(), Some(3.0));
        }

        #[wasm_bindgen_test]
        async fn sql_dump_import_cancels_after_chunk_error() {
            use std::cell::Cell;
            use wasm_bindgen::prelude::Closure;

            let cancelled = Rc::new(Cell::new(false));
            let cancelled_flag = Rc::clone(&cancelled);
            let cancel = Closure::wrap(Box::new(move |_id: String| -> JsValue {
                cancelled_flag.set(true);
                serde_wasm_bindgen::to_value(&WasmEncodedResult::success(String::new())).unwrap()
            }) as Box<dyn FnMut(String) -> JsValue>);
            let local_db = create_local_db(
                Some(create_success_callback("[]")),
                Some(success_wipe_callback()),
                Some(success_transaction_callback()),
            );
            Reflect::set(
                &local_db,
                &JsValue::from_str("beginSqlDumpImport"),
                &Function::new_no_args("return { value: 'session', error: null };"),
            )
            .unwrap();
            Reflect::set(
                &local_db,
                &JsValue::from_str("appendSqlDumpChunk"),
                &Function::new_no_args(
                    "return { value: null, error: { msg: 'bad SQL', readableMsg: 'bad SQL' } };",
                ),
            )
            .unwrap();
            Reflect::set(
                &local_db,
                &JsValue::from_str("cancelSqlDumpImport"),
                cancel.as_ref(),
            )
            .unwrap();
            cancel.forget();

            let exec = JsCallbackExecutor::new(local_db).unwrap();
            let result = exec
                .execute_sql_dump(Arc::new(
                    "BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string(),
                ))
                .await;
            assert!(result.is_err());
            assert!(cancelled.get());
        }

        #[wasm_bindgen_test]
        async fn dropped_import_cancels_before_releasing_executor_lock() {
            use gloo_timers::future::TimeoutFuture;
            use std::task::Poll;

            for stage in ["begin", "append", "finish"] {
                let local_db = create_local_db(
                    Some(create_success_callback("[]")),
                    Some(success_wipe_callback()),
                    Some(success_transaction_callback()),
                );
                for (method, current, value) in [
                    ("beginSqlDumpImport", "begin", "session"),
                    ("appendSqlDumpChunk", "append", ""),
                    ("finishSqlDumpImport", "finish", ""),
                ] {
                    let body = if current == stage {
                        "this.reached = true; this.inFlight = true; return new Promise(resolve => this.pendingResolve = resolve).then(value => { this.inFlight = false; return value; });".to_string()
                    } else {
                        format!("return {{ value: '{value}', error: null }};")
                    };
                    Reflect::set(
                        &local_db,
                        &JsValue::from_str(method),
                        &Function::new_no_args(&body),
                    )
                    .unwrap();
                }
                Reflect::set(
                    &local_db,
                    &JsValue::from_str("cancelSqlDumpImport"),
                    &Function::new_no_args(
                        "if (this.inFlight) throw new Error('Wait for the previous SQL dump import request to finish'); this.cancelled = true; return new Promise(resolve => this.cancelResolve = resolve);",
                    ),
                ).unwrap();
                let exec = JsCallbackExecutor::new(local_db.clone()).unwrap();
                let mut import = exec.execute_sql_dump(Arc::new(
                    "BEGIN; INSERT INTO t VALUES (1); COMMIT;".to_string(),
                ));
                for _ in 0..50 {
                    assert!(matches!(futures::poll!(import.as_mut()), Poll::Pending));
                    if Reflect::get(&local_db, &JsValue::from_str("reached"))
                        .unwrap()
                        .as_bool()
                        == Some(true)
                    {
                        break;
                    }
                    TimeoutFuture::new(0).await;
                }
                assert_eq!(
                    Reflect::get(&local_db, &JsValue::from_str("reached"))
                        .unwrap()
                        .as_bool(),
                    Some(true),
                    "stage {stage} must be reached"
                );
                drop(import);
                TimeoutFuture::new(0).await;
                assert_ne!(
                    Reflect::get(&local_db, &JsValue::from_str("cancelled"))
                        .unwrap()
                        .as_bool(),
                    Some(true)
                );
                let stmt = SqlStatement::new("SELECT 1");
                let mut query = exec.query_text(&stmt);
                assert!(matches!(futures::poll!(query.as_mut()), Poll::Pending));
                let resolve = method(&local_db, "pendingResolve").unwrap();
                resolve
                    .call1(
                        &local_db,
                        &serde_wasm_bindgen::to_value(&WasmEncodedResult::success(
                            if stage == "begin" { "session" } else { "" }.to_string(),
                        ))
                        .unwrap(),
                    )
                    .unwrap();
                TimeoutFuture::new(0).await;
                assert_eq!(
                    Reflect::get(&local_db, &JsValue::from_str("cancelled"))
                        .unwrap()
                        .as_bool(),
                    Some(true)
                );
                assert!(matches!(futures::poll!(query.as_mut()), Poll::Pending));
                let resolve = method(&local_db, "cancelResolve").unwrap();
                resolve
                    .call1(
                        &local_db,
                        &serde_wasm_bindgen::to_value(&WasmEncodedResult::success(String::new()))
                            .unwrap(),
                    )
                    .unwrap();
                assert_eq!(query.await.unwrap(), "[]");
            }
        }
    }
}
