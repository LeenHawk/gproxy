//! Shared helpers for the libSQL persistence backend.

use serde_json::Value;

use crate::store::libsql::{QueryResult, arg_integer, arg_null, arg_text};

/// Bind an optional integer arg (NULL when `None`).
pub fn arg_opt_i64(v: Option<i64>) -> Value {
    match v {
        Some(n) => arg_integer(n),
        None => arg_null(),
    }
}

/// Bind an optional text arg (NULL when `None`).
pub fn arg_opt_text(v: Option<&str>) -> Value {
    match v {
        Some(s) => arg_text(s),
        None => arg_null(),
    }
}

/// Bind a boolean arg as INTEGER 0/1.
pub fn arg_bool(v: bool) -> Value {
    arg_integer(v as i64)
}

/// Bind an optional boolean arg as INTEGER 0/1 (NULL when `None`).
pub fn arg_opt_bool(v: Option<bool>) -> Value {
    match v {
        Some(b) => arg_bool(b),
        None => arg_null(),
    }
}

use crate::store::libsql::LibsqlClient;
use crate::store::persistence::libsql::row::Row;

/// Run a query and return all rows (typed-value cells).
pub async fn query(client: &LibsqlClient, sql: &str, args: &[Value]) -> anyhow::Result<Vec<Row>> {
    let qr = client
        .execute(sql, args)
        .await
        .map_err(|e| anyhow::anyhow!("libsql query failed: {e}"))?;
    Ok(qr.rows)
}

/// Run a query and return the first row, if any.
pub async fn query_one(
    client: &LibsqlClient,
    sql: &str,
    args: &[Value],
) -> anyhow::Result<Option<Row>> {
    Ok(query(client, sql, args).await?.into_iter().next())
}

/// Run a write statement and return the affected-row count.
pub async fn exec(client: &LibsqlClient, sql: &str, args: &[Value]) -> anyhow::Result<u64> {
    let qr = client
        .execute(sql, args)
        .await
        .map_err(|e| anyhow::anyhow!("libsql exec failed: {e}"))?;
    Ok(qr.affected_row_count)
}

/// Current Unix time in seconds via the JS clock (wasm32 has no `Instant`).
pub fn now_secs() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

/// Parse the `last_insert_rowid` of a write result into an `i64`.
pub fn last_rowid(qr: &QueryResult) -> anyhow::Result<i64> {
    qr.last_insert_rowid
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("libsql: missing last_insert_rowid"))?
        .parse::<i64>()
        .map_err(|e| anyhow::anyhow!("libsql: bad last_insert_rowid: {e}"))
}
