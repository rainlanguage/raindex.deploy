use crate::local_db::query::{
    create_tables::REQUIRED_TABLES, LocalDbQueryExecutor, SqlStatement, SqlValue,
};
use crate::local_db::LocalDbError;
use alloy::primitives::Address;
use itertools::Itertools;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use super::RaindexIdentifier;

const SKIPPED_TABLES: &[&str] = &[
    "db_metadata",
    "sync_status",
    "raw_events",
    "take_order_contexts",
    "context_values",
];
const MAX_INSERT_ROWS: usize = 256;
const MAX_INSERT_BYTES: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
struct TableInfoRow {
    name: String,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("Row is not an object")]
    RowNotObject,

    #[error("Row is missing expected column '{column}'")]
    MissingColumn { column: String },

    #[error("Missing target_watermarks row for chain {chain_id} raindex {raindex_address}")]
    MissingTargetWatermark {
        chain_id: u32,
        raindex_address: Address,
    },
}

/// Export all data rows for a specific `(chain_id, raindex_address)` from the
/// local database as a SQL string containing data-only `INSERT` statements.
///
/// Returns `Ok(None)` when no matching rows are found.
///
/// Tables that do not include both `chain_id` and `raindex_address` columns,
/// or that are explicitly skipped via [`SKIPPED_TABLES`], are ignored.
pub async fn export_data_only<E>(
    executor: &E,
    raindex_id: &RaindexIdentifier,
) -> Result<Option<String>, LocalDbError>
where
    E: LocalDbQueryExecutor + ?Sized,
{
    let table_names = REQUIRED_TABLES
        .iter()
        .copied()
        .filter(|table| !SKIPPED_TABLES.contains(table));

    let mut inserts = String::new();
    let mut had_rows = false;

    for table in table_names {
        let columns = fetch_table_columns(executor, table).await?;
        if !has_target_filters(&columns) {
            continue;
        }

        let select_stmt = build_select_statement(table, &columns, raindex_id);
        let rows: Vec<Value> = executor
            .query_json(&select_stmt)
            .await
            .map_err(LocalDbError::from)?;
        if rows.is_empty() {
            continue;
        }

        let table_sql = build_insert_statements(table, &columns, &rows)?;
        if !had_rows {
            inserts.push_str("BEGIN;\n");
            had_rows = true;
        }
        inserts.push_str(&table_sql);
    }

    if had_rows {
        inserts.push_str("COMMIT;\n");
        Ok(Some(inserts))
    } else {
        Ok(None)
    }
}

async fn fetch_table_columns<E>(
    executor: &E,
    table: &str,
) -> Result<Vec<TableInfoRow>, LocalDbError>
where
    E: LocalDbQueryExecutor + ?Sized,
{
    let pragma_sql = format!("PRAGMA table_info('{table}')");
    let stmt = SqlStatement::new(pragma_sql);
    executor
        .query_json::<Vec<TableInfoRow>>(&stmt)
        .await
        .map_err(LocalDbError::from)
}

fn has_target_filters(columns: &[TableInfoRow]) -> bool {
    let has_chain = columns
        .iter()
        .any(|col| col.name.eq_ignore_ascii_case("chain_id"));
    let has_raindex = columns
        .iter()
        .any(|col| col.name.eq_ignore_ascii_case("raindex_address"));
    has_chain && has_raindex
}

fn build_select_statement(
    table: &str,
    columns: &[TableInfoRow],
    raindex_id: &RaindexIdentifier,
) -> SqlStatement {
    let columns_sql = columns.iter().map(|c| format!("\"{}\"", c.name)).join(", ");

    let mut order_columns: Vec<String> = Vec::new();
    if columns
        .iter()
        .any(|c| c.name.eq_ignore_ascii_case("chain_id"))
    {
        order_columns.push("\"chain_id\"".to_string());
    }
    if columns
        .iter()
        .any(|c| c.name.eq_ignore_ascii_case("raindex_address"))
    {
        order_columns.push("\"raindex_address\"".to_string());
    }

    for column in columns {
        let name = format!("\"{}\"", column.name);
        if !order_columns.iter().any(|c| c == &name) {
            order_columns.push(name);
        }
    }

    let order_clause = order_columns.join(", ");
    let mut stmt = SqlStatement::new(format!(
        "SELECT {columns_sql} FROM \"{table}\" WHERE chain_id = ?1 AND raindex_address = ?2 ORDER BY {order_clause};"
    ));
    stmt.push(SqlValue::from(raindex_id.chain_id as u64));
    stmt.push(SqlValue::from(raindex_id.raindex_address));
    stmt
}

fn build_insert_statements(
    table: &str,
    columns: &[TableInfoRow],
    rows: &[Value],
) -> Result<String, LocalDbError> {
    build_insert_statements_bounded(table, columns, rows, MAX_INSERT_ROWS, MAX_INSERT_BYTES)
}

fn build_insert_statements_bounded(
    table: &str,
    columns: &[TableInfoRow],
    rows: &[Value],
    max_rows: usize,
    max_bytes: usize,
) -> Result<String, LocalDbError> {
    let mut output = String::new();
    let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    let quoted_columns = column_names
        .iter()
        .map(|name| format!("\"{}\"", name))
        .join(", ");
    let prefix = format!("INSERT INTO \"{table}\" ({quoted_columns}) VALUES ");
    let mut statement = String::new();
    let mut statement_rows = 0;

    for row in rows {
        let values_sql = format_row_values(row, &column_names).map_err(LocalDbError::from)?;
        let tuple = format!("({values_sql})");
        // A single oversized row still needs its own statement to preserve the dump.
        if statement_rows > 0
            && (statement_rows == max_rows || statement.len() + 2 + tuple.len() + 2 > max_bytes)
        {
            statement.push_str(";\n");
            output.push_str(&statement);
            statement.clear();
            statement_rows = 0;
        }
        if statement_rows == 0 {
            statement.push_str(&prefix);
        } else {
            statement.push_str(", ");
        }
        statement.push_str(&tuple);
        statement_rows += 1;
    }

    if statement_rows > 0 {
        statement.push_str(";\n");
        output.push_str(&statement);
    }

    Ok(output)
}

fn format_row_values(row: &Value, columns: &[&str]) -> Result<String, ExportError> {
    let obj = row.as_object().ok_or(ExportError::RowNotObject)?;

    let formatted = columns
        .iter()
        .map(|col| {
            let value = obj.get(*col).ok_or_else(|| ExportError::MissingColumn {
                column: (*col).to_string(),
            })?;
            format_sql_value(value)
        })
        .collect::<Result<Vec<String>, ExportError>>()?;

    Ok(formatted.join(", "))
}

fn format_sql_value(value: &Value) -> Result<String, ExportError> {
    match value {
        Value::Null => Ok("NULL".to_string()),
        Value::Bool(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => Ok(format!("'{}'", s.replace('\'', "''"))),
        other => {
            let json = other.to_string().replace('\'', "''");
            Ok(format!("'{}'", json))
        }
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use crate::local_db::query::{
        create_tables::CREATE_TABLES_SQL, LocalDbQueryError, SqlStatementBatch, SqlValue,
    };
    use alloy::hex;
    use alloy::primitives::{hex::encode_prefixed, Address};
    use async_trait::async_trait;
    use futures::executor;
    use rusqlite::{params, types::ValueRef, Connection};
    use serde_json::{json, Map};
    use std::str::FromStr;
    use std::sync::Mutex;

    fn column(name: &str) -> TableInfoRow {
        TableInfoRow {
            name: name.to_string(),
        }
    }

    struct TestExecutor {
        conn: Mutex<Connection>,
    }

    impl TestExecutor {
        fn new() -> Self {
            let conn = Connection::open_in_memory().expect("open sqlite memory db");
            conn.execute_batch(CREATE_TABLES_SQL)
                .expect("create tables");

            seed_tables(&conn);

            Self {
                conn: Mutex::new(conn),
            }
        }
    }

    fn sqlvalue_to_rusqlite(v: SqlValue) -> rusqlite::types::Value {
        match v {
            SqlValue::Text(t) => rusqlite::types::Value::Text(t),
            SqlValue::I64(i) => rusqlite::types::Value::Integer(i),
            SqlValue::U64(u) => rusqlite::types::Value::Integer(u as i64),
            SqlValue::Null => rusqlite::types::Value::Null,
        }
    }

    #[cfg_attr(target_family = "wasm", async_trait(?Send))]
    #[cfg_attr(not(target_family = "wasm"), async_trait)]
    impl LocalDbQueryExecutor for TestExecutor {
        async fn execute_batch(&self, batch: &SqlStatementBatch) -> Result<(), LocalDbQueryError> {
            let conn = self.conn.lock().unwrap();
            for stmt in batch {
                if stmt.params().is_empty() {
                    conn.execute_batch(stmt.sql()).map_err(|e| {
                        LocalDbQueryError::database(format!("SQL execution failed: {e}"))
                    })?;
                } else {
                    let mut prepared = conn.prepare(stmt.sql()).map_err(|e| {
                        LocalDbQueryError::database(format!("Failed to prepare query: {e}"))
                    })?;
                    let params = rusqlite::params_from_iter(
                        stmt.params().iter().cloned().map(sqlvalue_to_rusqlite),
                    );
                    prepared.execute(params).map_err(|e| {
                        LocalDbQueryError::database(format!("SQL execution failed: {e}"))
                    })?;
                }
            }
            Ok(())
        }

        async fn query_json<T>(&self, stmt: &SqlStatement) -> Result<T, LocalDbQueryError>
        where
            T: crate::local_db::query::FromDbJson,
        {
            let conn = self.conn.lock().unwrap();
            let mut prepared = conn.prepare(stmt.sql()).map_err(|e| {
                LocalDbQueryError::database(format!("Failed to prepare query: {e}"))
            })?;
            let column_names: Vec<String> = (0..prepared.column_count())
                .map(|i| {
                    let raw = prepared.column_name(i).unwrap_or("");
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        format!("column_{i}")
                    } else {
                        trimmed.to_string()
                    }
                })
                .collect();

            let params =
                rusqlite::params_from_iter(stmt.params().iter().cloned().map(sqlvalue_to_rusqlite));

            let rows_iter = prepared
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

            let mut out = Vec::new();
            for row in rows_iter {
                out.push(row.map_err(|e| LocalDbQueryError::database(format!("Row error: {e}")))?);
            }

            let json_value = Value::Array(out);
            serde_json::from_value::<T>(json_value)
                .map_err(|e| LocalDbQueryError::deserialization(e.to_string()))
        }

        async fn query_text(&self, stmt: &SqlStatement) -> Result<String, LocalDbQueryError> {
            let conn = self.conn.lock().unwrap();
            if stmt.params().is_empty() {
                conn.execute_batch(stmt.sql()).map_err(|e| {
                    LocalDbQueryError::database(format!("SQL execution failed: {e}"))
                })?;
            } else {
                let mut prepared = conn.prepare(stmt.sql()).map_err(|e| {
                    LocalDbQueryError::database(format!("Failed to prepare query: {e}"))
                })?;
                let params = rusqlite::params_from_iter(
                    stmt.params().iter().cloned().map(sqlvalue_to_rusqlite),
                );
                prepared.execute(params).map_err(|e| {
                    LocalDbQueryError::database(format!("SQL execution failed: {e}"))
                })?;
            }
            Ok(String::new())
        }

        async fn wipe_and_recreate(&self) -> Result<(), LocalDbQueryError> {
            Err(LocalDbQueryError::not_implemented("wipe_and_recreate"))
        }
    }

    struct TestTarget {
        chain_id: i64,
        raindex: Address,
        label: &'static str,
    }

    fn seed_tables(conn: &Connection) {
        let specs = [
            TestTarget {
                chain_id: 42161,
                raindex: Address::from_str("0x0000000000000000000000000000000000000aaa").unwrap(),
                label: "main",
            },
            TestTarget {
                chain_id: 42161,
                raindex: Address::from_str("0x0000000000000000000000000000000000000bbb").unwrap(),
                label: "alt",
            },
            TestTarget {
                chain_id: 10,
                raindex: Address::from_str("0x0000000000000000000000000000000000000ccc").unwrap(),
                label: "other",
            },
        ];

        for (idx, spec) in specs.iter().enumerate() {
            insert_for_target(conn, spec, (idx as i64 + 1) * 10);
        }
    }

    fn insert_for_target(conn: &Connection, target: &TestTarget, base_idx: i64) {
        let chain = target.chain_id;
        let raindex = encode_prefixed(target.raindex);
        let label = target.label;

        let raw_tx = format!("raw_tx_{label}");
        conn.execute(
            r#"INSERT INTO raw_events (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                address,
                topics,
                data,
                raw_json
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10
            );"#,
            params![
                chain,
                raindex.as_str(),
                raw_tx,
                base_idx,
                1000 + base_idx,
                1_700_000_000 + base_idx,
                format!("address_{label}"),
                format!("[\"topic_{label}\"]"),
                format!("data_{label}"),
                format!("{{\"event\":\"raw_{label}\"}}"),
            ],
        )
        .expect("insert raw_events");

        let deposit_tx = format!("dep_tx_{label}");
        conn.execute(
            r#"INSERT INTO deposits (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                sender,
                token,
                vault_id,
                deposit_amount,
                deposit_amount_uint256
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11
            );"#,
            params![
                chain,
                raindex.as_str(),
                deposit_tx,
                base_idx + 1,
                1000 + base_idx + 1,
                1_700_000_000 + base_idx + 1,
                format!("sender_{label}"),
                format!("token_{label}"),
                format!("vault_{label}"),
                format!("amount_{label}"),
                format!("uint_{label}"),
            ],
        )
        .expect("insert deposits");

        let withdraw_tx = format!("with_tx_{label}");
        conn.execute(
            r#"INSERT INTO withdrawals (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                sender,
                token,
                vault_id,
                target_amount,
                withdraw_amount,
                withdraw_amount_uint256
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11,
                ?12
            );"#,
            params![
                chain,
                raindex.as_str(),
                withdraw_tx,
                base_idx + 2,
                1000 + base_idx + 2,
                1_700_000_000 + base_idx + 2,
                format!("withdraw_sender_{label}"),
                format!("withdraw_token_{label}"),
                format!("withdraw_vault_{label}"),
                format!("target_amount_{label}"),
                format!("withdraw_amount_{label}"),
                format!("withdraw_uint_{label}"),
            ],
        )
        .expect("insert withdrawals");

        let order_tx = format!("order_tx_{label}");
        conn.execute(
            r#"INSERT INTO order_events (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                sender,
                interpreter_address,
                store_address,
                order_hash,
                event_type,
                order_owner,
                order_nonce,
                order_bytes
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11,
                ?12,
                ?13,
                ?14
            );"#,
            params![
                chain,
                raindex.as_str(),
                order_tx,
                base_idx + 3,
                1000 + base_idx + 3,
                1_700_000_000 + base_idx + 3,
                format!("order_sender_{label}"),
                format!("interp_{label}"),
                format!("store_{label}"),
                format!("order_hash_{label}"),
                format!("event_{label}"),
                format!("order_owner_{label}"),
                format!("order_nonce_{label}"),
                format!("order_bytes_{label}"),
            ],
        )
        .expect("insert order_events");

        conn.execute(
            r#"INSERT INTO order_ios (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                io_index,
                io_type,
                token,
                vault_id
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8
            );"#,
            params![
                chain,
                raindex.as_str(),
                order_tx,
                base_idx + 3,
                0,
                format!("io_type_{label}"),
                format!("io_token_{label}"),
                format!("io_vault_{label}"),
            ],
        )
        .expect("insert order_ios");

        let take_tx = format!("take_tx_{label}");
        conn.execute(
            r#"INSERT INTO take_orders (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                sender,
                order_owner,
                order_nonce,
                input_io_index,
                output_io_index,
                taker_input,
                taker_output
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11,
                ?12,
                ?13
            );"#,
            params![
                chain,
                raindex.as_str(),
                take_tx,
                base_idx + 4,
                1000 + base_idx + 4,
                1_700_000_000 + base_idx + 4,
                format!("taker_{label}"),
                format!("taker_owner_{label}"),
                format!("taker_nonce_{label}"),
                0,
                1,
                format!("taker_input_{label}"),
                format!("taker_output_{label}"),
            ],
        )
        .expect("insert take_orders");

        conn.execute(
            r#"INSERT INTO take_order_contexts (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                context_index,
                context_value
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6
            );"#,
            params![
                chain,
                raindex.as_str(),
                take_tx,
                base_idx + 4,
                0,
                format!("context_entry_{label}"),
            ],
        )
        .expect("insert take_order_contexts");

        conn.execute(
            r#"INSERT INTO context_values (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                context_index,
                value_index,
                value
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7
            );"#,
            params![
                chain,
                raindex.as_str(),
                take_tx,
                base_idx + 4,
                0,
                0,
                format!("context_value_{label}"),
            ],
        )
        .expect("insert context_values");

        let clear_tx = format!("clear_tx_{label}");
        conn.execute(
            r#"INSERT INTO clear_v3_events (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                sender,
                alice_order_hash,
                alice_order_owner,
                alice_input_io_index,
                alice_output_io_index,
                alice_bounty_vault_id,
                alice_input_vault_id,
                alice_output_vault_id,
                bob_order_hash,
                bob_order_owner,
                bob_input_io_index,
                bob_output_io_index,
                bob_bounty_vault_id,
                bob_input_vault_id,
                bob_output_vault_id
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11,
                ?12,
                ?13,
                ?14,
                ?15,
                ?16,
                ?17,
                ?18,
                ?19,
                ?20,
                ?21
            );"#,
            params![
                chain,
                raindex.as_str(),
                clear_tx,
                base_idx + 5,
                1000 + base_idx + 5,
                1_700_000_000 + base_idx + 5,
                format!("clear_sender_{label}"),
                format!("alice_hash_{label}"),
                format!("alice_owner_{label}"),
                0,
                1,
                format!("alice_bounty_{label}"),
                format!("alice_input_{label}"),
                format!("alice_output_{label}"),
                format!("bob_hash_{label}"),
                format!("bob_owner_{label}"),
                2,
                3,
                format!("bob_bounty_{label}"),
                format!("bob_input_{label}"),
                format!("bob_output_{label}"),
            ],
        )
        .expect("insert clear_v3_events");

        conn.execute(
            r#"INSERT INTO after_clear_v2_events (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                sender,
                alice_output,
                bob_output,
                alice_input,
                bob_input
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11
            );"#,
            params![
                chain,
                raindex.as_str(),
                clear_tx,
                base_idx + 6,
                1000 + base_idx + 6,
                1_700_000_000 + base_idx + 6,
                format!("after_sender_{label}"),
                format!("alice_output_amount_{label}"),
                format!("bob_output_amount_{label}"),
                format!("alice_input_amount_{label}"),
                format!("bob_input_amount_{label}"),
            ],
        )
        .expect("insert after_clear_v2_events");

        let meta_tx = format!("meta_tx_{label}");
        conn.execute(
            r#"INSERT INTO meta_events (
                chain_id,
                raindex_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                sender,
                subject,
                meta
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9
            );"#,
            params![
                chain,
                raindex.as_str(),
                meta_tx,
                base_idx + 7,
                1000 + base_idx + 7,
                1_700_000_000 + base_idx + 7,
                format!("meta_sender_{label}"),
                format!("subject_{label}"),
                format!("0x{}", hex::encode(label.as_bytes())),
            ],
        )
        .expect("insert meta_events");

        conn.execute(
            r#"INSERT INTO erc20_tokens (
                chain_id,
                raindex_address,
                token_address,
                name,
                symbol,
                decimals
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6
            );"#,
            params![
                chain,
                raindex.as_str(),
                format!("token_addr_{label}"),
                format!("Token {label}"),
                format!("SYM{label}"),
                18,
            ],
        )
        .expect("insert erc20_tokens");

        let store_tx = format!("store_tx_{label}");
        conn.execute(
            r#"INSERT INTO interpreter_store_sets (
                chain_id,
                raindex_address,
                store_address,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                namespace,
                key,
                value
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10
            );"#,
            params![
                chain,
                raindex.as_str(),
                format!("store_addr_{label}"),
                store_tx,
                base_idx + 8,
                1000 + base_idx + 8,
                1_700_000_000 + base_idx + 8,
                format!("store_namespace_{label}"),
                format!("store_key_{label}"),
                format!("store_value_{label}"),
            ],
        )
        .expect("insert interpreter_store_sets");

        conn.execute(
            r#"INSERT INTO derived_trades (
                chain_id,
                raindex_address,
                trade_id,
                trade_kind,
                trade_side,
                order_hash,
                order_owner,
                order_nonce,
                transaction_hash,
                log_index,
                block_number,
                block_timestamp,
                transaction_sender,
                input_vault_id,
                input_token,
                input_delta,
                input_running_balance,
                output_vault_id,
                output_token,
                output_delta,
                output_running_balance
            ) VALUES (
                ?1,
                ?2,
                ?3,
                'take',
                'take',
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11,
                ?12,
                ?13,
                ?14,
                ?15,
                ?16,
                ?17,
                ?18,
                ?19
            );"#,
            params![
                chain,
                raindex.as_str(),
                format!("trade_id_{label}"),
                format!("derived_order_hash_{label}"),
                format!("derived_owner_{label}"),
                format!("derived_nonce_{label}"),
                format!("derived_tx_{label}"),
                base_idx + 9,
                1000 + base_idx + 9,
                1_700_000_000 + base_idx + 9,
                format!("derived_sender_{label}"),
                format!("derived_input_vault_{label}"),
                format!("derived_input_token_{label}"),
                format!("derived_input_delta_{label}"),
                format!("derived_input_balance_{label}"),
                format!("derived_output_vault_{label}"),
                format!("derived_output_token_{label}"),
                format!("derived_output_delta_{label}"),
                format!("derived_output_balance_{label}"),
            ],
        )
        .expect("insert derived_trades");

        conn.execute(
            r#"INSERT INTO target_watermarks (
                chain_id,
                raindex_address,
                last_block,
                last_hash,
                updated_at
            ) VALUES (
                ?1,
                ?2,
                ?3,
                ?4,
                ?5
            );"#,
            params![
                chain,
                raindex.as_str(),
                2_000 + base_idx,
                format!("hash_{label}"),
                1_700_000_000_000i64 + base_idx * 1_000,
            ],
        )
        .expect("insert target_watermarks");
    }

    #[tokio::test]
    async fn export_includes_only_targeted_rows() {
        let executor = TestExecutor::new();
        let main_target = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: Address::from_str("0x0000000000000000000000000000000000000aaa")
                .unwrap(),
        };
        let alt_target = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: Address::from_str("0x0000000000000000000000000000000000000bbb")
                .unwrap(),
        };
        let other_target = RaindexIdentifier {
            chain_id: 10,
            raindex_address: Address::from_str("0x0000000000000000000000000000000000000ccc")
                .unwrap(),
        };

        let sql_main = export_data_only(&executor, &main_target)
            .await
            .unwrap()
            .expect("main target should have rows");
        let sql_alt = export_data_only(&executor, &alt_target)
            .await
            .unwrap()
            .expect("alt target should have rows");
        let sql_other = export_data_only(&executor, &other_target)
            .await
            .unwrap()
            .expect("other target should have rows");

        let expected_main = expected_dump(42161, main_target.raindex_address, "main", 10);
        let expected_alt = expected_dump(42161, alt_target.raindex_address, "alt", 20);
        let expected_other = expected_dump(10, other_target.raindex_address, "other", 30);

        let norm = |s: &str| normalize_sql(s).to_lowercase();
        assert_eq!(norm(&sql_main), norm(&expected_main), "main dump mismatch");
        assert_eq!(norm(&sql_alt), norm(&expected_alt), "alt dump mismatch");
        assert_eq!(
            norm(&sql_other),
            norm(&expected_other),
            "other dump mismatch"
        );
    }

    #[tokio::test]
    async fn export_returns_none_when_no_rows() {
        let executor = TestExecutor::new();
        let raindex_id = RaindexIdentifier {
            chain_id: 1,
            raindex_address: Address::from_str("0x0000000000000000000000000000000000000ddd")
                .unwrap(),
        };

        let export = export_data_only(&executor, &raindex_id).await.unwrap();
        assert!(
            export.is_none(),
            "expected None when there are no rows for the target"
        );
    }

    #[test]
    fn export_omits_skipped_tables() {
        let executor = TestExecutor::new();
        let raindex_id = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: Address::from_str("0x0000000000000000000000000000000000000aaa")
                .unwrap(),
        };
        let raindex = encode_prefixed(raindex_id.raindex_address);

        {
            let conn = executor.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO db_metadata (id, db_schema_version) VALUES (?1, ?2);",
                params![1, 999],
            )
            .expect("insert db_metadata row");
            conn.execute(
                "INSERT INTO sync_status (chain_id, raindex_address, last_synced_block) VALUES (?1, ?2, ?3);",
                params![raindex_id.chain_id as i64, raindex.as_str(), 456_i64],
            )
            .expect("insert sync_status row");
        }

        let sql = executor::block_on(export_data_only(&executor, &raindex_id))
            .unwrap()
            .expect("target should have rows");
        let expected = expected_dump(42161, raindex_id.raindex_address, "main", 10);

        assert_eq!(
            normalize_sql(&sql),
            normalize_sql(&expected),
            "unexpected rows exported with skipped tables present"
        );
        assert!(
            sql.contains("INSERT INTO \"target_watermarks\""),
            "target_watermarks rows should be exported"
        );
        for table in SKIPPED_TABLES {
            assert!(
                !sql.contains(table),
                "dump includes rows for unexpectedly skipped table {table}"
            );
        }
    }

    #[test]
    fn has_target_filters_detects_required_columns() {
        let columns = vec![column("chain_id"), column("raindex_address"), column("foo")];
        assert!(has_target_filters(&columns));

        let missing_raindex = vec![column("chain_id"), column("foo")];
        assert!(!has_target_filters(&missing_raindex));

        let missing_chain = vec![column("raindex_address"), column("foo")];
        assert!(!has_target_filters(&missing_chain));
    }

    #[test]
    fn build_select_statement_orders_columns_and_params() {
        let columns = vec![
            column("raindex_address"),
            column("chain_id"),
            column("alpha"),
            column("beta"),
        ];
        let raindex_id = RaindexIdentifier {
            chain_id: 42161,
            raindex_address: Address::from_str("0x00112233445566778899aabbccddeeff00112233")
                .unwrap(),
        };

        let stmt = build_select_statement("deposits", &columns, &raindex_id);
        assert_eq!(
            stmt.sql(),
            "SELECT \"raindex_address\", \"chain_id\", \"alpha\", \"beta\" FROM \"deposits\" WHERE chain_id = ?1 AND raindex_address = ?2 ORDER BY \"chain_id\", \"raindex_address\", \"alpha\", \"beta\";"
        );
        let params = stmt.params();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], SqlValue::from(raindex_id.chain_id as u64));
        assert_eq!(params[1], SqlValue::from(raindex_id.raindex_address));
    }

    #[test]
    fn format_row_values_handles_multiple_types() {
        let mut row_map = Map::new();
        row_map.insert("null_val".to_string(), Value::Null);
        row_map.insert("bool_true".to_string(), Value::Bool(true));
        row_map.insert("bool_false".to_string(), Value::Bool(false));
        row_map.insert(
            "number".to_string(),
            Value::Number(serde_json::Number::from(42)),
        );
        row_map.insert("text".to_string(), Value::String("O'Malley".to_string()));
        row_map.insert("array".to_string(), json!([1, 2]));
        row_map.insert("object".to_string(), json!({"nested": "value"}));

        let formatted = format_row_values(
            &Value::Object(row_map),
            &[
                "null_val",
                "bool_true",
                "bool_false",
                "number",
                "text",
                "array",
                "object",
            ],
        )
        .unwrap();
        assert_eq!(
            formatted,
            "NULL, 1, 0, 42, 'O''Malley', '[1,2]', '{\"nested\":\"value\"}'"
        );
    }

    #[test]
    fn format_sql_value_quotes_and_serializes() {
        assert_eq!(format_sql_value(&Value::Null).unwrap(), "NULL");
        assert_eq!(format_sql_value(&Value::Bool(true)).unwrap(), "1");
        assert_eq!(format_sql_value(&Value::Bool(false)).unwrap(), "0");
        assert_eq!(
            format_sql_value(&Value::Number(serde_json::Number::from(7))).unwrap(),
            "7"
        );
        assert_eq!(
            format_sql_value(&Value::String("foo'bar".into())).unwrap(),
            "'foo''bar'"
        );
        assert_eq!(
            format_sql_value(&json!({"alpha": 1})).unwrap(),
            "'{\"alpha\":1}'"
        );
    }

    #[test]
    fn format_row_values_missing_column_errors() {
        let mut row_map = Map::new();
        row_map.insert("present".to_string(), Value::String("value".into()));

        let err = format_row_values(&Value::Object(row_map), &["present", "missing"])
            .expect_err("expected missing column to error");
        match err {
            ExportError::MissingColumn { column } => {
                assert_eq!(column, "missing");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn grouped_inserts_preserve_rows_and_bound_statement_size() {
        let rows = vec![
            json!({"value": "first"}),
            json!({"value": "O'Malley"}),
            json!({"value": "third"}),
            json!({"value": "fourth"}),
            json!({"value": "last"}),
        ];
        let columns = [column("value")];
        let by_rows = build_insert_statements_bounded("items", &columns, &rows, 2, usize::MAX)
            .expect("group rows");
        assert_eq!(by_rows.lines().count(), 3);
        assert_eq!(by_rows.matches("INSERT INTO").count(), 3);

        let by_bytes = build_insert_statements_bounded("items", &columns, &rows, 256, 75)
            .expect("bound statement bytes");
        assert!(by_bytes.lines().count() > 1);
        assert!(by_bytes.lines().all(|line| line.len() < 75));

        for sql in [by_rows, by_bytes] {
            let conn = Connection::open_in_memory().expect("open database");
            conn.execute_batch("CREATE TABLE items (value TEXT NOT NULL);")
                .expect("create table");
            conn.execute_batch(&sql).expect("import grouped inserts");
            let values: Vec<String> = conn
                .prepare("SELECT value FROM items ORDER BY rowid")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(values, ["first", "O'Malley", "third", "fourth", "last"]);
        }
    }

    #[test]
    fn exported_inserts_split_at_default_row_limit() {
        let rows = (0..=MAX_INSERT_ROWS)
            .map(|value| json!({"value": value}))
            .collect::<Vec<_>>();
        let sql = build_insert_statements("items", &[column("value")], &rows).expect("export rows");
        assert_eq!(sql.lines().count(), 2);
        assert_eq!(sql.lines().next().unwrap().matches("), (").count(), 255);
    }

    fn expected_dump(chain_id: u32, raindex: Address, label: &str, base_idx: i64) -> String {
        let base = base_idx;
        let dep_idx = base + 1;
        let with_idx = base + 2;
        let order_idx = base + 3;
        let take_idx = base + 4;
        let clear_idx = base + 5;
        let after_idx = base + 6;
        let meta_idx = base + 7;
        let store_idx = base + 8;

        let block = |offset: i64| 1000 + offset;
        let ts = |offset: i64| 1_700_000_000 + offset;

        let meta_hex = format!("0x{}", hex::encode(label.as_bytes()));
        let raindex = encode_prefixed(raindex);
        let watermark_block = 2_000 + base_idx;
        let watermark_ms = 1_700_000_000_000i64 + base_idx * 1_000;

        let mut out = String::from("BEGIN;\n");

        out.push_str(&format!(
            "INSERT INTO \"target_watermarks\" (\"chain_id\", \"raindex_address\", \"last_block\", \"last_hash\", \"updated_at\") VALUES ({}, '{}', {}, 'hash_{}', {});\n",
            chain_id,
            raindex,
            watermark_block,
            label,
            watermark_ms
        ));

        out.push_str(&format!(
            "INSERT INTO \"deposits\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"sender\", \"token\", \"vault_id\", \"deposit_amount\", \"deposit_amount_uint256\") VALUES ({}, '{}', 'dep_tx_{}', {}, {}, {}, 'sender_{}', 'token_{}', 'vault_{}', 'amount_{}', 'uint_{}');\n",
            chain_id,
            raindex,
            label,
            dep_idx,
            block(dep_idx),
            ts(dep_idx),
            label,
            label,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"withdrawals\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"sender\", \"token\", \"vault_id\", \"target_amount\", \"withdraw_amount\", \"withdraw_amount_uint256\") VALUES ({}, '{}', 'with_tx_{}', {}, {}, {}, 'withdraw_sender_{}', 'withdraw_token_{}', 'withdraw_vault_{}', 'target_amount_{}', 'withdraw_amount_{}', 'withdraw_uint_{}');\n",
            chain_id,
            raindex,
            label,
            with_idx,
            block(with_idx),
            ts(with_idx),
            label,
            label,
            label,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"order_events\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"sender\", \"interpreter_address\", \"store_address\", \"order_hash\", \"event_type\", \"order_owner\", \"order_nonce\", \"order_bytes\") VALUES ({}, '{}', 'order_tx_{}', {}, {}, {}, 'order_sender_{}', 'interp_{}', 'store_{}', 'order_hash_{}', 'event_{}', 'order_owner_{}', 'order_nonce_{}', 'order_bytes_{}');\n",
            chain_id,
            raindex,
            label,
            order_idx,
            block(order_idx),
            ts(order_idx),
            label,
            label,
            label,
            label,
            label,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"order_ios\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"io_index\", \"io_type\", \"token\", \"vault_id\") VALUES ({}, '{}', 'order_tx_{}', {}, 0, 'io_type_{}', 'io_token_{}', 'io_vault_{}');\n",
            chain_id,
            raindex,
            label,
            order_idx,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"take_orders\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"sender\", \"order_owner\", \"order_nonce\", \"input_io_index\", \"output_io_index\", \"taker_input\", \"taker_output\") VALUES ({}, '{}', 'take_tx_{}', {}, {}, {}, 'taker_{}', 'taker_owner_{}', 'taker_nonce_{}', 0, 1, 'taker_input_{}', 'taker_output_{}');\n",
            chain_id,
            raindex,
            label,
            take_idx,
            block(take_idx),
            ts(take_idx),
            label,
            label,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"clear_v3_events\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"sender\", \"alice_order_hash\", \"alice_order_owner\", \"alice_input_io_index\", \"alice_output_io_index\", \"alice_bounty_vault_id\", \"alice_input_vault_id\", \"alice_output_vault_id\", \"bob_order_hash\", \"bob_order_owner\", \"bob_input_io_index\", \"bob_output_io_index\", \"bob_bounty_vault_id\", \"bob_input_vault_id\", \"bob_output_vault_id\") VALUES ({}, '{}', 'clear_tx_{}', {}, {}, {}, 'clear_sender_{}', 'alice_hash_{}', 'alice_owner_{}', 0, 1, 'alice_bounty_{}', 'alice_input_{}', 'alice_output_{}', 'bob_hash_{}', 'bob_owner_{}', 2, 3, 'bob_bounty_{}', 'bob_input_{}', 'bob_output_{}');\n",
            chain_id,
            raindex,
            label,
            clear_idx,
            block(clear_idx),
            ts(clear_idx),
            label,
            label,
            label,
            label,
            label,
            label,
            label,
            label,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"after_clear_v2_events\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"sender\", \"alice_output\", \"bob_output\", \"alice_input\", \"bob_input\") VALUES ({}, '{}', 'clear_tx_{}', {}, {}, {}, 'after_sender_{}', 'alice_output_amount_{}', 'bob_output_amount_{}', 'alice_input_amount_{}', 'bob_input_amount_{}');\n",
            chain_id,
            raindex,
            label,
            after_idx,
            block(after_idx),
            ts(after_idx),
            label,
            label,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"meta_events\" (\"chain_id\", \"raindex_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"sender\", \"subject\", \"meta\") VALUES ({}, '{}', 'meta_tx_{}', {}, {}, {}, 'meta_sender_{}', 'subject_{}', '{}');\n",
            chain_id,
            raindex,
            label,
            meta_idx,
            block(meta_idx),
            ts(meta_idx),
            label,
            label,
            meta_hex
        ));

        out.push_str(&format!(
            "INSERT INTO \"erc20_tokens\" (\"chain_id\", \"raindex_address\", \"token_address\", \"name\", \"symbol\", \"decimals\") VALUES ({}, '{}', 'token_addr_{}', 'Token {}', 'SYM{}', 18);\n",
            chain_id,
            raindex,
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"interpreter_store_sets\" (\"chain_id\", \"raindex_address\", \"store_address\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"namespace\", \"key\", \"value\") VALUES ({}, '{}', 'store_addr_{}', 'store_tx_{}', {}, {}, {}, 'store_namespace_{}', 'store_key_{}', 'store_value_{}');\n",
            chain_id,
            raindex,
            label,
            label,
            store_idx,
            block(store_idx),
            ts(store_idx),
            label,
            label,
            label
        ));

        out.push_str(&format!(
            "INSERT INTO \"derived_trades\" (\"chain_id\", \"raindex_address\", \"trade_id\", \"trade_kind\", \"trade_side\", \"order_hash\", \"order_owner\", \"order_nonce\", \"transaction_hash\", \"log_index\", \"block_number\", \"block_timestamp\", \"transaction_sender\", \"input_vault_id\", \"input_token\", \"input_delta\", \"input_running_balance\", \"output_vault_id\", \"output_token\", \"output_delta\", \"output_running_balance\") VALUES ({}, '{}', 'trade_id_{}', 'take', 'take', 'derived_order_hash_{}', 'derived_owner_{}', 'derived_nonce_{}', 'derived_tx_{}', {}, {}, {}, 'derived_sender_{}', 'derived_input_vault_{}', 'derived_input_token_{}', 'derived_input_delta_{}', 'derived_input_balance_{}', 'derived_output_vault_{}', 'derived_output_token_{}', 'derived_output_delta_{}', 'derived_output_balance_{}');\n",
            chain_id,
            raindex,
            label,
            label,
            label,
            label,
            label,
            base + 9,
            block(base + 9),
            ts(base + 9),
            label,
            label,
            label,
            label,
            label,
            label,
            label,
            label,
            label
        ));

        out.push_str("COMMIT;\n");
        out
    }

    fn normalize_sql(input: &str) -> String {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum PrevKind {
            Word,
            Quoted,
            RightParen,
            Other,
        }

        fn flush_current(current: &mut String, out: &mut String, prev_kind: &mut Option<PrevKind>) {
            if current.is_empty() {
                return;
            }
            if matches!(
                prev_kind,
                Some(PrevKind::Word | PrevKind::Quoted | PrevKind::RightParen)
            ) {
                out.push(' ');
            }
            out.push_str(current);
            current.clear();
            *prev_kind = Some(PrevKind::Word);
        }

        let mut out = String::new();
        let mut current = String::new();
        let mut prev_kind: Option<PrevKind> = None;
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\'' | '"' => {
                    flush_current(&mut current, &mut out, &mut prev_kind);
                    let mut quoted = String::from(ch);
                    while let Some(next) = chars.next() {
                        quoted.push(next);
                        if next == ch {
                            if let Some(&lookahead) = chars.peek() {
                                if lookahead == ch {
                                    quoted.push(chars.next().unwrap());
                                    continue;
                                }
                            }
                            break;
                        }
                    }
                    if matches!(
                        prev_kind,
                        Some(PrevKind::Word | PrevKind::Quoted | PrevKind::RightParen)
                    ) {
                        out.push(' ');
                    }
                    out.push_str(&quoted);
                    prev_kind = Some(PrevKind::Quoted);
                }
                '(' => {
                    flush_current(&mut current, &mut out, &mut prev_kind);
                    if out.ends_with(' ') {
                        out.pop();
                    }
                    out.push('(');
                    prev_kind = Some(PrevKind::Other);
                }
                ')' => {
                    flush_current(&mut current, &mut out, &mut prev_kind);
                    out.push(')');
                    prev_kind = Some(PrevKind::RightParen);
                }
                ',' => {
                    flush_current(&mut current, &mut out, &mut prev_kind);
                    out.push(',');
                    prev_kind = Some(PrevKind::Other);
                }
                ';' => {
                    flush_current(&mut current, &mut out, &mut prev_kind);
                    out.push_str(";\n");
                    prev_kind = None;
                }
                c if c.is_whitespace() => {
                    flush_current(&mut current, &mut out, &mut prev_kind);
                }
                other => current.push(other),
            }
        }

        flush_current(&mut current, &mut out, &mut prev_kind);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}
