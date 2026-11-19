use super::functions;
use super::query::create_tables::RAINDEX_APPLICATION_ID;
use super::query::{
    FromDbJson, LocalDbQueryError, LocalDbQueryExecutor, SqlStatement, SqlStatementBatch, SqlValue,
};
use async_trait::async_trait;
use raindex_app_settings::local_db_manifest::DB_SCHEMA_VERSION;
use rusqlite::{types::ValueRef, Connection};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tokio::sync::RwLock as AsyncRwLock;
use tokio::task::spawn_blocking;

/// Pool of idle SQLite connections, each already set up with WAL journal mode,
/// a busy timeout, and the registered custom functions. Reusing them avoids
/// paying the full `open_connection` setup cost on every query, which matters
/// for bursts of concurrent order lookups.
type ConnectionPool = Arc<Mutex<Vec<Connection>>>;

/// SQLite permits concurrent readers in WAL mode, but still permits only one
/// writer. Keep one access coordinator per database path so independent
/// executors cannot race writes and database recreation can invalidate every
/// connection pool before unlinking the database.
static DB_PATH_STATES: OnceLock<Mutex<HashMap<PathBuf, Weak<DbPathState>>>> = OnceLock::new();

struct DbPathState {
    access: Arc<AsyncRwLock<()>>,
    pools: Mutex<Vec<Weak<Mutex<Vec<Connection>>>>>,
}

impl DbPathState {
    /// Registers a connection pool that must be invalidated if this database is
    /// recreated. Dead pools are removed whenever a new executor is registered.
    fn register_pool(&self, pool: &ConnectionPool) {
        let mut pools = self.pools.lock().unwrap_or_else(|e| e.into_inner());
        pools.retain(|pool| pool.upgrade().is_some());
        pools.push(Arc::downgrade(pool));
    }

    /// Drops every idle connection for this database path. Callers must hold
    /// the exclusive access guard so no connection can still be checked out.
    fn clear_pools(&self) {
        let mut pools = self.pools.lock().unwrap_or_else(|e| e.into_inner());
        pools.retain(|pool| {
            if let Some(pool) = pool.upgrade() {
                pool.lock().unwrap_or_else(|e| e.into_inner()).clear();
                true
            } else {
                false
            }
        });
    }
}

pub struct RusqliteExecutor {
    db_path: PathBuf,
    pool: ConnectionPool,
    db_state: Arc<DbPathState>,
}

fn sqlvalue_to_rusqlite(v: SqlValue) -> rusqlite::types::Value {
    match v {
        SqlValue::Text(t) => rusqlite::types::Value::Text(t),
        SqlValue::I64(i) => rusqlite::types::Value::Integer(i),
        SqlValue::U64(u) => match i64::try_from(u) {
            Ok(i) => rusqlite::types::Value::Integer(i),
            Err(_) => rusqlite::types::Value::Text(u.to_string()),
        },
        SqlValue::Null => rusqlite::types::Value::Null,
    }
}

impl RusqliteExecutor {
    pub fn new<P: AsRef<Path>>(db_path: P) -> Self {
        let db_path = db_path.as_ref().to_path_buf();
        let pool = Arc::new(Mutex::new(Vec::new()));
        let db_state = db_path_state_for(&db_path);
        db_state.register_pool(&pool);
        Self {
            db_path,
            pool,
            db_state,
        }
    }

    #[cfg(test)]
    fn pool_len(&self) -> usize {
        self.pool.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    fn invoke_statement(conn: &Connection, stmt: &SqlStatement) -> Result<(), LocalDbQueryError> {
        if stmt.params().is_empty() {
            conn.execute_batch(stmt.sql())
                .map_err(|e| LocalDbQueryError::database(format!("SQL execution failed: {e}")))?;
        } else {
            let mut prepared = conn.prepare(stmt.sql()).map_err(|e| {
                LocalDbQueryError::database(format!("Failed to prepare query: {e}"))
            })?;
            let bound = stmt.params().iter().cloned().map(sqlvalue_to_rusqlite);
            let params = rusqlite::params_from_iter(bound);
            prepared
                .execute(params)
                .map_err(|e| LocalDbQueryError::database(format!("SQL execution failed: {e}")))?;
        }
        Ok(())
    }
}

/// Returns the shared access coordinator for a database path, pruning entries
/// whose executors have all been dropped before looking up the requested path.
fn db_path_state_for(db_path: &Path) -> Arc<DbPathState> {
    let key = db_path_state_key(db_path);
    let states = DB_PATH_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut states = states.lock().unwrap_or_else(|e| e.into_inner());
    states.retain(|_, state| state.upgrade().is_some());

    if let Some(state) = states.get(&key).and_then(Weak::upgrade) {
        return state;
    }

    let state = Arc::new(DbPathState {
        access: Arc::new(AsyncRwLock::new(())),
        pools: Mutex::new(Vec::new()),
    });
    states.insert(key, Arc::downgrade(&state));
    state
}

/// Produces a stable absolute identity for an existing database or for a new
/// database whose parent directory already exists.
fn db_path_state_key(db_path: &Path) -> PathBuf {
    if let Ok(path) = std::fs::canonicalize(db_path) {
        return path;
    }

    let absolute = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(db_path))
            .unwrap_or_else(|_| db_path.to_path_buf())
    };

    match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(file_name)) => std::fs::canonicalize(parent)
            .map(|parent| parent.join(file_name))
            .unwrap_or(absolute),
        _ => absolute,
    }
}

/// Take a ready-to-use connection: reuse an idle one from the pool if available,
/// otherwise open (and fully set up) a fresh one. The mutex is held only for the
/// brief pop, never across the open or the query, so concurrency is preserved.
fn checkout(pool: &ConnectionPool, db_path: &Path) -> Result<Connection, LocalDbQueryError> {
    let pooled = pool.lock().unwrap_or_else(|e| e.into_inner()).pop();
    match pooled {
        Some(conn) => Ok(conn),
        None => open_connection(db_path),
    }
}

/// Return a healthy connection to the pool for reuse. Only call this after the
/// connection's work succeeded; a connection left in a bad state (e.g. mid
/// failed transaction) must be dropped rather than released.
fn release(pool: &ConnectionPool, conn: Connection) {
    pool.lock().unwrap_or_else(|e| e.into_inner()).push(conn);
}

fn open_connection(db_path: &Path) -> Result<Connection, LocalDbQueryError> {
    let conn = Connection::open(db_path)
        .map_err(|e| LocalDbQueryError::database(format!("Failed to open database: {e}")))?;
    conn.pragma_update(None, "journal_mode", "wal")
        .map_err(|e| LocalDbQueryError::database(format!("Failed to set WAL journal mode: {e}")))?;
    conn.busy_timeout(Duration::from_millis(500))
        .map_err(|e| LocalDbQueryError::database(format!("Failed to set busy_timeout: {e}")))?;
    functions::register_all(&conn).map_err(|e| {
        LocalDbQueryError::database(format!("Failed to register sqlite functions: {e}"))
    })?;
    verify_schema_guard(&conn)?;
    Ok(conn)
}

/// Structural schema-version guard read from the SQLite file header on every
/// connection open. `application_id` labels the file as a raindex local-db and
/// `user_version` carries the schema version; both are stamped by
/// `create_tables` (see `query.sql`) and survive table corruption.
///
/// A brand-new file reports `application_id == 0` before any tables are
/// created, so it is allowed through unverified to let bootstrap create and
/// stamp the schema. Once stamped, a foreign file (wrong `application_id`) or a
/// stale-version file (wrong `user_version`) is rejected up front rather than
/// surfacing later as an opaque `no such column` error.
fn verify_schema_guard(conn: &Connection) -> Result<(), LocalDbQueryError> {
    let app_id: i32 = conn
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(|e| LocalDbQueryError::database(format!("Failed to read application_id: {e}")))?;

    // An unstamped, freshly-created database header reads zero. Defer to
    // bootstrap, which creates the tables and stamps the header.
    if app_id == 0 {
        return Ok(());
    }

    if app_id != RAINDEX_APPLICATION_ID {
        return Err(LocalDbQueryError::NotARaindexDb {
            expected: RAINDEX_APPLICATION_ID,
            found: app_id,
        });
    }

    let user_version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| LocalDbQueryError::database(format!("Failed to read user_version: {e}")))?;

    let expected = DB_SCHEMA_VERSION as i32;
    if user_version != expected {
        return Err(LocalDbQueryError::SchemaVersionMismatch {
            expected,
            found: user_version,
        });
    }

    Ok(())
}

fn join_err(err: tokio::task::JoinError) -> LocalDbQueryError {
    LocalDbQueryError::database(format!("Blocking task failed: {err}"))
}

#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
impl LocalDbQueryExecutor for RusqliteExecutor {
    async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
        if !batch.is_transaction() {
            return Err(LocalDbQueryError::database(
                "SQL statement batch must be wrapped in a transaction",
            ));
        }

        let db_path = self.db_path.clone();
        let pool = self.pool.clone();
        let write_guard = Arc::clone(&self.db_state.access).write_owned().await;
        let batch = batch.clone();
        spawn_blocking(move || {
            let _write_guard = write_guard;
            let conn = checkout(&pool, &db_path)?;
            for stmt in &batch {
                if let Err(err) = RusqliteExecutor::invoke_statement(&conn, stmt) {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(err);
                }
            }
            release(&pool, conn);
            Ok(())
        })
        .await
        .map_err(join_err)?
    }

    async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
        let db_path = self.db_path.clone();
        let pool = self.pool.clone();
        let write_guard = Arc::clone(&self.db_state.access).write_owned().await;
        let stmt = stmt.clone();
        spawn_blocking(move || {
            let _write_guard = write_guard;
            let conn = checkout(&pool, &db_path)?;
            RusqliteExecutor::invoke_statement(&conn, &stmt)?;
            release(&pool, conn);
            Ok(String::new())
        })
        .await
        .map_err(join_err)?
    }

    async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
    where
        T: FromDbJson,
    {
        let db_path = self.db_path.clone();
        let pool = self.pool.clone();
        let read_guard = Arc::clone(&self.db_state.access).read_owned().await;
        let stmt = stmt.clone();

        let json_value = spawn_blocking(move || {
            let _read_guard = read_guard;
            let conn = checkout(&pool, &db_path)?;
            let mut s = conn.prepare(stmt.sql()).map_err(|e| {
                LocalDbQueryError::database(format!("Failed to prepare query: {e}"))
            })?;
            if !s.readonly() {
                return Err(LocalDbQueryError::database(
                    "query_json only supports read-only SQL statements",
                ));
            }
            let column_names: Vec<String> = (0..s.column_count())
                .map(|i| {
                    let raw = s.column_name(i).unwrap_or("");
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        format!("column_{}", i)
                    } else {
                        trimmed.to_string()
                    }
                })
                .collect();

            let bound = stmt.params().iter().cloned().map(sqlvalue_to_rusqlite);
            let params = rusqlite::params_from_iter(bound);

            let rows_iter = s
                .query_map(params, |row| {
                    let mut obj = Map::with_capacity(column_names.len());
                    for (i, name) in column_names.iter().enumerate() {
                        let v = match row.get_ref(i)? {
                            ValueRef::Null => Value::Null,
                            ValueRef::Integer(n) => json!(n),
                            ValueRef::Real(f) => json!(f),
                            ValueRef::Text(bytes) => match std::str::from_utf8(bytes) {
                                Ok(s) => json!(s),
                                Err(_) => json!(alloy::hex::encode_prefixed(bytes)),
                            },
                            ValueRef::Blob(bytes) => json!(alloy::hex::encode_prefixed(bytes)),
                        };
                        obj.insert(name.clone(), v);
                    }
                    Ok(Value::Object(obj))
                })
                .map_err(|e| LocalDbQueryError::database(format!("Query failed: {e}")))?;

            let mut out: Vec<Value> = Vec::new();
            for r in rows_iter {
                let v = r.map_err(|e| LocalDbQueryError::database(format!("Row error: {e}")))?;
                out.push(v);
            }

            // Drop the prepared statement (which borrows `conn`) before returning
            // the connection to the pool for reuse.
            drop(s);
            release(&pool, conn);

            Ok::<_, LocalDbQueryError>(Value::Array(out))
        })
        .await
        .map_err(join_err)??;

        serde_json::from_value::<T>(json_value)
            .map_err(|e| LocalDbQueryError::deserialization(e.to_string()))
    }

    async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
        let db_path = self.db_path.clone();
        let db_state = Arc::clone(&self.db_state);
        let write_guard = Arc::clone(&db_state.access).write_owned().await;
        spawn_blocking(move || {
            let _write_guard = write_guard;
            db_state.clear_pools();

            for path in sqlite_file_paths(&db_path) {
                if path.exists() {
                    std::fs::remove_file(&path).map_err(|e| {
                        LocalDbQueryError::database(format!(
                            "Failed to delete database file {}: {e}",
                            path.display()
                        ))
                    })?;
                }
            }
            let conn = open_connection(&db_path)?;
            drop(conn);
            Ok(())
        })
        .await
        .map_err(join_err)?
    }
}

fn sqlite_file_paths(db_path: &Path) -> Vec<PathBuf> {
    ["", "-wal", "-shm"]
        .into_iter()
        .map(|suffix| {
            let mut path = db_path.as_os_str().to_os_string();
            path.push(suffix);
            PathBuf::from(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_db::query::create_tables::{create_tables_batch, create_tables_sql};
    use tempfile::TempDir;

    #[test]
    fn executors_for_same_database_share_path_state() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("shared-write-gate.db");
        let first = RusqliteExecutor::new(&db_path);
        let second = RusqliteExecutor::new(&db_path);

        assert!(Arc::ptr_eq(&first.db_state, &second.db_state));
    }

    #[tokio::test]
    async fn write_operations_wait_for_shared_database_gate() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("serialized-writes.db");
        let first = RusqliteExecutor::new(&db_path);
        let second = RusqliteExecutor::new(&db_path);
        let guard = Arc::clone(&first.db_state.access).write_owned().await;

        let statement = SqlStatement::new("CREATE TABLE serialized_write (id INTEGER);");
        let mut pending_write = Box::pin(second.query_text(&statement));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), pending_write.as_mut())
                .await
                .is_err(),
            "a second executor must wait while the database write gate is held"
        );

        drop(guard);
        tokio::time::timeout(Duration::from_secs(5), pending_write)
            .await
            .expect("write should resume after the gate is released")
            .expect("write should succeed");
    }

    #[tokio::test]
    async fn query_json_rejects_row_returning_writes() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("read-only-json.db");
        let exec = RusqliteExecutor::new(&db_path);
        exec.query_text(&SqlStatement::new("CREATE TABLE items (id INTEGER);"))
            .await
            .unwrap();

        #[derive(Debug, serde::Deserialize)]
        struct ReturnedId {
            #[allow(dead_code)]
            id: i64,
        }

        let err = exec
            .query_json::<Vec<ReturnedId>>(&SqlStatement::new(
                "INSERT INTO items (id) VALUES (1) RETURNING id;",
            ))
            .await
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("query_json only supports read-only SQL statements"));

        #[derive(serde::Deserialize)]
        struct RowCount {
            count: i64,
        }

        let rows: Vec<RowCount> = exec
            .query_json(&SqlStatement::new("SELECT COUNT(*) AS count FROM items;"))
            .await
            .unwrap();
        assert_eq!(rows[0].count, 0);
    }

    #[tokio::test]
    async fn wipe_invalidates_every_pool_for_database_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("shared-pools.db");
        let first = RusqliteExecutor::new(&db_path);
        let second = RusqliteExecutor::new(&db_path);
        first
            .query_text(&SqlStatement::new("CREATE TABLE items (id INTEGER);"))
            .await
            .unwrap();

        #[derive(serde::Deserialize)]
        struct RowCount {
            #[allow(dead_code)]
            count: i64,
        }

        let _: Vec<RowCount> = second
            .query_json(&SqlStatement::new("SELECT COUNT(*) AS count FROM items;"))
            .await
            .unwrap();
        assert_eq!(first.pool_len(), 1);
        assert_eq!(second.pool_len(), 1);

        first.wipe_and_recreate().await.unwrap();

        assert_eq!(first.pool_len(), 0);
        assert_eq!(second.pool_len(), 0);
        second
            .query_text(&SqlStatement::new("CREATE TABLE recreated (id INTEGER);"))
            .await
            .unwrap();
    }

    #[test]
    fn empty_db_passes_schema_guard() {
        // A brand-new connection has application_id == 0 and must be allowed
        // through so bootstrap can create and stamp the schema.
        let conn = Connection::open_in_memory().unwrap();
        verify_schema_guard(&conn).expect("unstamped db should pass the guard");
    }

    #[test]
    fn stamped_db_passes_schema_guard() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(create_tables_sql())
            .expect("create tables stamps the header");
        verify_schema_guard(&conn).expect("correctly stamped db should pass the guard");
    }

    #[test]
    fn wrong_application_id_is_detected() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(create_tables_sql())
            .expect("create tables stamps the header");
        // Re-stamp the header with a foreign application_id (a SQLite file that
        // is not a raindex local-db) while leaving user_version valid.
        let foreign = RAINDEX_APPLICATION_ID + 1;
        conn.pragma_update(None, "application_id", foreign)
            .expect("override application_id");

        match verify_schema_guard(&conn) {
            Err(LocalDbQueryError::NotARaindexDb { expected, found }) => {
                assert_eq!(expected, RAINDEX_APPLICATION_ID);
                assert_eq!(found, foreign);
            }
            other => panic!("expected NotARaindexDb, got {other:?}"),
        }
    }

    #[test]
    fn negative_application_id_is_detected() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(create_tables_sql())
            .expect("create tables stamps the header");
        // A foreign file whose application_id has the top bit set reads back as
        // a negative i32. It is neither zero (unstamped) nor the raindex magic,
        // so it must be rejected as NotARaindexDb rather than waved through.
        let foreign = i32::MIN;
        conn.pragma_update(None, "application_id", foreign)
            .expect("override application_id");

        match verify_schema_guard(&conn) {
            Err(LocalDbQueryError::NotARaindexDb { expected, found }) => {
                assert_eq!(expected, RAINDEX_APPLICATION_ID);
                assert_eq!(found, foreign);
            }
            other => panic!("expected NotARaindexDb, got {other:?}"),
        }
    }

    #[test]
    fn wrong_user_version_is_detected() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(create_tables_sql())
            .expect("create tables stamps the header");
        // Keep the raindex application_id but advance user_version to a stale /
        // future schema number.
        let bumped = DB_SCHEMA_VERSION as i32 + 1;
        conn.pragma_update(None, "user_version", bumped)
            .expect("override user_version");

        match verify_schema_guard(&conn) {
            Err(LocalDbQueryError::SchemaVersionMismatch { expected, found }) => {
                assert_eq!(expected, DB_SCHEMA_VERSION as i32);
                assert_eq!(found, bumped);
            }
            other => panic!("expected SchemaVersionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn stale_user_version_is_detected() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(create_tables_sql())
            .expect("create tables stamps the header");
        // Keep the raindex application_id but roll user_version back to an older
        // schema number. A stale (lower) version must be rejected just like a
        // future (higher) one.
        let stale = DB_SCHEMA_VERSION as i32 - 1;
        conn.pragma_update(None, "user_version", stale)
            .expect("override user_version");

        match verify_schema_guard(&conn) {
            Err(LocalDbQueryError::SchemaVersionMismatch { expected, found }) => {
                assert_eq!(expected, DB_SCHEMA_VERSION as i32);
                assert_eq!(found, stale);
            }
            other => panic!("expected SchemaVersionMismatch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn open_rejects_foreign_application_id_db() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("foreign.db");

        // Build a stamped raindex db, then corrupt only the application_id label
        // in the file header so a fresh open must reject it.
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(create_tables_sql()).unwrap();
            conn.pragma_update(None, "application_id", RAINDEX_APPLICATION_ID + 7)
                .unwrap();
        }

        let exec = RusqliteExecutor::new(&db_path);
        let err = exec
            .query_text(&SqlStatement::new("SELECT 1;"))
            .await
            .expect_err("open of a foreign db must fail");
        assert!(
            matches!(err, LocalDbQueryError::NotARaindexDb { .. }),
            "expected NotARaindexDb, got {err:?}"
        );
    }

    #[tokio::test]
    async fn open_rejects_stale_user_version_db() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("stale.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(create_tables_sql()).unwrap();
            conn.pragma_update(None, "user_version", DB_SCHEMA_VERSION as i32 + 1)
                .unwrap();
        }

        let exec = RusqliteExecutor::new(&db_path);
        let err = exec
            .query_text(&SqlStatement::new("SELECT 1;"))
            .await
            .expect_err("open of a stale-version db must fail");
        assert!(
            matches!(err, LocalDbQueryError::SchemaVersionMismatch { .. }),
            "expected SchemaVersionMismatch, got {err:?}"
        );
    }

    #[tokio::test]
    async fn create_tables_stamps_header_and_open_succeeds() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("stamped.db");

        let exec = RusqliteExecutor::new(&db_path);
        // First open hits an empty file (application_id == 0) and stamps it.
        exec.execute_batch(&create_tables_batch()).await.unwrap();

        // A subsequent open must read back the stamped header and succeed.
        #[derive(serde::Deserialize)]
        struct VersionRow {
            user_version: i64,
        }
        let rows: Vec<VersionRow> = exec
            .query_json(&SqlStatement::new("PRAGMA user_version;"))
            .await
            .unwrap();
        assert_eq!(rows[0].user_version, DB_SCHEMA_VERSION as i64);

        let conn = Connection::open(&db_path).unwrap();
        let app_id: i32 = conn
            .query_row("PRAGMA application_id", [], |row| row.get(0))
            .unwrap();
        assert_eq!(app_id, RAINDEX_APPLICATION_ID);
    }

    #[tokio::test]
    async fn data_only_dump_apply_preserves_header() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("dump.db");

        let exec = RusqliteExecutor::new(&db_path);
        exec.execute_batch(&create_tables_batch()).await.unwrap();

        // Apply a data-only insert batch (the shape produced by export_data_only).
        let mut batch = SqlStatementBatch::new();
        batch.add(SqlStatement::new(
            "INSERT INTO db_metadata (id, db_schema_version) VALUES (1, 5);",
        ));
        let batch = batch.ensure_transaction();
        exec.execute_batch(&batch).await.unwrap();

        // The header guard must still pass after applying data rows.
        exec.query_text(&SqlStatement::new("SELECT 1;"))
            .await
            .expect("open after dump apply should still pass the guard");

        let conn = Connection::open(&db_path).unwrap();
        let app_id: i32 = conn
            .query_row("PRAGMA application_id", [], |row| row.get(0))
            .unwrap();
        let user_version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(app_id, RAINDEX_APPLICATION_ID);
        assert_eq!(user_version, DB_SCHEMA_VERSION as i32);
    }

    #[tokio::test]
    async fn execute_batch_runs_all_statements() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("batch.db");
        let db_path_str = db_path.to_string_lossy();

        let exec = RusqliteExecutor::new(&*db_path_str);

        let mut batch = SqlStatementBatch::new();
        batch.add(SqlStatement::new(
            "CREATE TABLE widgets (name TEXT, qty INTEGER);",
        ));

        let mut param_insert =
            SqlStatement::new("INSERT INTO widgets (name, qty) VALUES (?1, ?2);");
        param_insert.push("widget-a");
        param_insert.push(5i64);
        batch.add(param_insert);

        batch.add(SqlStatement::new(
            "INSERT INTO widgets (name, qty) VALUES ('widget-b', 7);",
        ));

        let batch = batch.ensure_transaction();

        exec.execute_batch(&batch).await.unwrap();

        #[derive(serde::Deserialize)]
        struct CountRow {
            total: i64,
        }
        let count_rows: Vec<CountRow> = exec
            .query_json(&SqlStatement::new("SELECT COUNT(*) AS total FROM widgets;"))
            .await
            .unwrap();
        assert_eq!(count_rows[0].total, 2);

        #[derive(serde::Deserialize)]
        struct WidgetRow {
            qty: i64,
        }
        let widget_rows: Vec<WidgetRow> = exec
            .query_json(&SqlStatement::new(
                "SELECT qty FROM widgets WHERE name = 'widget-a';",
            ))
            .await
            .unwrap();
        assert_eq!(widget_rows[0].qty, 5);
    }

    #[tokio::test]
    async fn execute_and_query_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_string_lossy();

        let exec = RusqliteExecutor::new(&*db_path_str);
        exec.query_text(&SqlStatement::new(
            "CREATE TABLE numbers (n INTEGER); INSERT INTO numbers (n) VALUES (1), (2);",
        ))
        .await
        .unwrap();

        #[derive(serde::Deserialize)]
        struct CountRow {
            c: i64,
        }
        let rows: Vec<CountRow> = exec
            .query_json(&SqlStatement::new("SELECT COUNT(*) AS c FROM numbers;"))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].c, 2);
    }

    #[tokio::test]
    async fn detects_required_tables() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("schema.db");
        let db_path_str = db_path.to_string_lossy();

        let exec = RusqliteExecutor::new(&*db_path_str);
        exec.query_text(&SqlStatement::new(
            "CREATE TABLE foo (id INTEGER); CREATE TABLE bar (id INTEGER);",
        ))
        .await
        .unwrap();

        #[derive(serde::Deserialize)]
        struct TableNameRow {
            name: String,
        }
        const TABLE_QUERY: &str =
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%';";
        let rows: Vec<TableNameRow> = exec
            .query_json(&SqlStatement::new(TABLE_QUERY))
            .await
            .unwrap();
        let existing: std::collections::HashSet<String> = rows
            .into_iter()
            .map(|row| row.name.to_ascii_lowercase())
            .collect();

        let has = |req: &[&str]| {
            req.iter()
                .all(|t| existing.contains(&t.to_ascii_lowercase()))
        };
        assert!(has(&["foo", "bar"]));
        assert!(!has(&["foo", "baz"]));
    }

    #[tokio::test]
    async fn query_json_multi_column_rows() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("multi.db");
        let db_path_str = db_path.to_string_lossy();

        let exec = RusqliteExecutor::new(&*db_path_str);
        exec.query_text(&SqlStatement::new(
            r#"
                CREATE TABLE people (
                    id INTEGER,
                    name TEXT,
                    note BLOB
                );
                INSERT INTO people (id, name, note) VALUES
                    (1, 'Alice', X'000102'),
                    (2, 'Bob',   X'ff'),
                    (3, 'Carol', NULL),
                    (4, 'Дора',  X'c0'),
                    (5, 'Eve',   X'');
                "#,
        ))
        .await
        .unwrap();

        #[derive(serde::Deserialize, Debug, PartialEq, Eq)]
        struct PersonRow {
            id: i64,
            name: String,
            note: Option<String>,
        }

        let rows: Vec<PersonRow> = exec
            .query_json(&SqlStatement::new(
                "SELECT id, name, note FROM people ORDER BY id ASC;",
            ))
            .await
            .unwrap();

        assert_eq!(rows.len(), 5);
        assert_eq!(
            rows,
            vec![
                PersonRow {
                    id: 1,
                    name: "Alice".to_string(),
                    note: Some("0x000102".to_string()),
                },
                PersonRow {
                    id: 2,
                    name: "Bob".to_string(),
                    note: Some("0xff".to_string()),
                },
                PersonRow {
                    id: 3,
                    name: "Carol".to_string(),
                    note: None,
                },
                PersonRow {
                    id: 4,
                    name: "Дора".to_string(),
                    note: Some("0xc0".to_string()),
                },
                PersonRow {
                    id: 5,
                    name: "Eve".to_string(),
                    note: Some("0x".to_string()),
                },
            ]
        );
    }

    #[tokio::test]
    async fn binds_u64_params_as_integer_and_text() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("u64.db");
        let db_path_str = db_path.to_string_lossy();

        let exec = RusqliteExecutor::new(&*db_path_str);
        exec.query_text(&SqlStatement::new(
            "CREATE TABLE uvals (label TEXT PRIMARY KEY, val);",
        ))
        .await
        .unwrap();

        let mut insert_small = SqlStatement::new("INSERT INTO uvals (label, val) VALUES (?1, ?2);");
        insert_small.push("small");
        insert_small.push(123u64);

        let large_value = (i64::MAX as u64) + 1;
        let mut insert_large = SqlStatement::new("INSERT INTO uvals (label, val) VALUES (?1, ?2);");
        insert_large.push("large");
        insert_large.push(large_value);

        let batch = SqlStatementBatch::from(vec![insert_small, insert_large]).ensure_transaction();
        exec.execute_batch(&batch).await.unwrap();

        #[derive(serde::Deserialize, Debug)]
        struct U64Row {
            label: String,
            ty: String,
            val: Value,
        }

        let rows: Vec<U64Row> = exec
            .query_json(&SqlStatement::new(
                "SELECT label, typeof(val) AS ty, val FROM uvals ORDER BY label ASC;",
            ))
            .await
            .unwrap();

        assert_eq!(rows.len(), 2);

        let small = rows.iter().find(|row| row.label == "small").unwrap();
        assert_eq!(small.ty, "integer");
        assert_eq!(small.val, json!(123));

        let large = rows.iter().find(|row| row.label == "large").unwrap();
        assert_eq!(large.ty, "text");
        assert_eq!(large.val, json!(large_value.to_string()));
    }

    #[tokio::test]
    async fn execute_batch_rolls_back_on_error_non_parameterized() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("rollback-non-param.db");
        let db_path_str = db_path.to_string_lossy();
        let exec = RusqliteExecutor::new(&*db_path_str);

        let mut setup_batch = SqlStatementBatch::new();
        setup_batch.add(SqlStatement::new(
            "CREATE TABLE rollback_non_param (id INTEGER PRIMARY KEY, value TEXT NOT NULL);",
        ));
        let setup_batch = setup_batch.ensure_transaction();
        exec.execute_batch(&setup_batch).await.unwrap();

        let mut batch = SqlStatementBatch::new();
        batch.add(SqlStatement::new(
            "INSERT INTO rollback_non_param (id, value) VALUES (1, 'first');",
        ));
        batch.add(SqlStatement::new(
            "INSERT INTO rollback_non_param (id, value) VALUES (1, 'duplicate');",
        ));
        let batch = batch.ensure_transaction();

        let err = exec.execute_batch(&batch).await.unwrap_err();
        assert!(matches!(err, LocalDbQueryError::Database { .. }));

        let conn = Connection::open(&*db_path_str).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rollback_non_param;", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn execute_batch_rolls_back_on_error_parameterized() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("rollback-param.db");
        let db_path_str = db_path.to_string_lossy();
        let exec = RusqliteExecutor::new(&*db_path_str);

        let mut setup_batch = SqlStatementBatch::new();
        setup_batch.add(SqlStatement::new(
            "CREATE TABLE rollback_param (id INTEGER PRIMARY KEY, value TEXT NOT NULL);",
        ));
        let setup_batch = setup_batch.ensure_transaction();
        exec.execute_batch(&setup_batch).await.unwrap();

        let mut insert_ok =
            SqlStatement::new("INSERT INTO rollback_param (id, value) VALUES (?1, ?2);");
        insert_ok.push(1i64);
        insert_ok.push("first");

        let mut insert_fail =
            SqlStatement::new("INSERT INTO rollback_param (id, value) VALUES (?1, ?2);");
        insert_fail.push(1i64);
        insert_fail.push("duplicate");

        let mut batch = SqlStatementBatch::new();
        batch.add(insert_ok);
        batch.add(insert_fail);
        let batch = batch.ensure_transaction();

        let err = exec.execute_batch(&batch).await.unwrap_err();
        assert!(matches!(err, LocalDbQueryError::Database { .. }));

        let conn = Connection::open(&*db_path_str).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rollback_param;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn execute_batch_requires_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("batch.db");
        let db_path_str = db_path.to_string_lossy();

        let exec = RusqliteExecutor::new(&*db_path_str);

        let batch = SqlStatementBatch::from(vec![SqlStatement::new("SELECT 1")]);
        let err = exec.execute_batch(&batch).await.unwrap_err();

        assert!(matches!(err, LocalDbQueryError::Database { .. }));
    }

    #[tokio::test]
    async fn wipe_and_recreate_removes_and_recreates_db() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("wipe.db");
        let db_path_str = db_path.to_string_lossy();

        let exec = RusqliteExecutor::new(&*db_path_str);
        exec.query_text(&SqlStatement::new("CREATE TABLE test_table (id INTEGER);"))
            .await
            .unwrap();

        assert!(db_path.exists());

        exec.wipe_and_recreate().await.unwrap();

        assert!(db_path.exists());

        #[derive(serde::Deserialize)]
        struct TableNameRow {
            #[allow(dead_code)]
            name: String,
        }
        let rows: Vec<TableNameRow> = exec
            .query_json(&SqlStatement::new(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%';",
            ))
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn sequential_queries_reuse_a_single_pooled_connection() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("pool-reuse.db");

        let exec = RusqliteExecutor::new(&db_path);

        // Nothing has run yet, so the pool starts empty.
        assert_eq!(exec.pool_len(), 0, "pool should start empty");

        // A query_text seeds the pool with one reusable connection.
        exec.query_text(&SqlStatement::new(
            "CREATE TABLE nums (n INTEGER); INSERT INTO nums (n) VALUES (10), (20), (30);",
        ))
        .await
        .unwrap();
        assert_eq!(
            exec.pool_len(),
            1,
            "successful query_text must return its connection to the pool"
        );

        #[derive(serde::Deserialize)]
        struct SumRow {
            total: i64,
        }

        // Run several sequential queries on the same executor. Each one must
        // check the single connection out and put it back, so the results stay
        // correct AND the pool never grows past one (proving reuse, not churn).
        for _ in 0..5 {
            let rows: Vec<SumRow> = exec
                .query_json(&SqlStatement::new("SELECT SUM(n) AS total FROM nums;"))
                .await
                .unwrap();
            assert_eq!(rows[0].total, 60);
            assert_eq!(
                exec.pool_len(),
                1,
                "sequential queries must reuse the one pooled connection"
            );
        }

        // execute_batch participates in the same pool on success.
        let mut batch = SqlStatementBatch::new();
        batch.add(SqlStatement::new("INSERT INTO nums (n) VALUES (40);"));
        let batch = batch.ensure_transaction();
        exec.execute_batch(&batch).await.unwrap();
        assert_eq!(
            exec.pool_len(),
            1,
            "successful execute_batch must return its connection to the pool"
        );

        let rows: Vec<SumRow> = exec
            .query_json(&SqlStatement::new("SELECT SUM(n) AS total FROM nums;"))
            .await
            .unwrap();
        assert_eq!(rows[0].total, 100);
    }

    #[tokio::test]
    async fn failed_batch_does_not_return_connection_to_pool() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("pool-fail.db");
        let exec = RusqliteExecutor::new(&db_path);

        let mut setup = SqlStatementBatch::new();
        setup.add(SqlStatement::new(
            "CREATE TABLE uniq (id INTEGER PRIMARY KEY, value TEXT NOT NULL);",
        ));
        let setup = setup.ensure_transaction();
        exec.execute_batch(&setup).await.unwrap();
        assert_eq!(exec.pool_len(), 1, "successful setup seeds the pool");

        // A batch that violates the primary key fails mid-transaction. It checks
        // the single pooled connection out, then drops it (rather than releasing
        // it) because it could be left in a bad state. The pool therefore ends
        // empty: the connection was taken and not returned.
        let mut batch = SqlStatementBatch::new();
        batch.add(SqlStatement::new(
            "INSERT INTO uniq (id, value) VALUES (1, 'first');",
        ));
        batch.add(SqlStatement::new(
            "INSERT INTO uniq (id, value) VALUES (1, 'duplicate');",
        ));
        let batch = batch.ensure_transaction();
        exec.execute_batch(&batch).await.unwrap_err();
        assert_eq!(
            exec.pool_len(),
            0,
            "a failed batch must drop its connection, not return it to the pool"
        );

        // The next successful query opens a fresh connection and seeds the pool
        // again, proving the executor recovers cleanly after a failure.
        exec.query_text(&SqlStatement::new("SELECT 1;"))
            .await
            .unwrap();
        assert_eq!(
            exec.pool_len(),
            1,
            "a successful query after a failure re-seeds the pool"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_pool_access_does_not_deadlock() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("concurrent-pool.db");

        let exec = std::sync::Arc::new(RusqliteExecutor::new(&db_path));

        // Seed a small table so the concurrent readers have real rows to return.
        exec.query_text(&SqlStatement::new(
            "CREATE TABLE nums (n INTEGER); INSERT INTO nums (n) VALUES (1), (2), (3), (4), (5);",
        ))
        .await
        .unwrap();

        #[derive(serde::Deserialize)]
        struct SumRow {
            total: i64,
        }

        // Fire a large number of concurrent operations through the real executor
        // API. Each one races to check a connection out of the shared pool, run
        // its query, and release it back, so the pool mutex is hammered under
        // heavy contention. A regression that held the lock across the query (or
        // otherwise deadlocked checkout/release) would hang here and trip the
        // timeout below rather than completing.
        let mut handles = Vec::new();
        for i in 0..200 {
            let exec = exec.clone();
            handles.push(tokio::spawn(async move {
                // Interleave a few writes among the many reads so checkout/release
                // is exercised from both query_json and execute_batch paths.
                if i % 25 == 0 {
                    let mut batch = SqlStatementBatch::new();
                    batch.add(SqlStatement::new("INSERT INTO nums (n) VALUES (0);"));
                    let batch = batch.ensure_transaction();
                    exec.execute_batch(&batch).await.unwrap();
                }
                let rows: Vec<SumRow> = exec
                    .query_json(&SqlStatement::new("SELECT SUM(n) AS total FROM nums;"))
                    .await
                    .unwrap();
                // The seeded rows sum to 15; the interleaved zero-inserts never
                // change that, so every read must observe at least the seed total.
                assert!(rows[0].total >= 15, "unexpected sum {}", rows[0].total);
            }));
        }

        // A deadlock or lock-held-across-query regression manifests as a hang, so
        // bound the whole fan-out and assert it finishes well inside the budget.
        let joined = futures::future::join_all(handles);
        let results = tokio::time::timeout(std::time::Duration::from_secs(30), joined)
            .await
            .expect("concurrent pool access must not deadlock (timed out)");

        for result in results {
            result.expect("a concurrent operation panicked");
        }
    }
}
