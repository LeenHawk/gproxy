//! Admin audit-log ops for the libSQL edge backend (append-only).

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_i64, col_opt_i64, col_opt_str, col_str};
use crate::store::persistence::libsql::util::{
    arg_opt_i64, arg_opt_text, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{AuditLog, AuditLogInput};

const COLS: &str = "id, at, actor_id, actor_name, action, target, status, source_ip, created_at";

fn decode(row: &Row) -> anyhow::Result<AuditLog> {
    Ok(AuditLog {
        id: col_i64(row, 0)?,
        at: col_i64(row, 1)?,
        actor_id: col_opt_i64(row, 2)?,
        actor_name: col_opt_str(row, 3)?,
        action: col_str(row, 4)?,
        target: col_str(row, 5)?,
        status: col_i64(row, 6)?,
        source_ip: col_opt_str(row, 7)?,
        created_at: col_i64(row, 8)?,
    })
}

pub async fn append(client: &LibsqlClient, input: AuditLogInput) -> anyhow::Result<AuditLog> {
    let now = now_secs();
    let qr = client
        .execute(
            "INSERT INTO audit_logs \
             (at, actor_id, actor_name, action, target, status, source_ip, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                arg_integer(now),
                arg_opt_i64(input.actor_id),
                arg_opt_text(input.actor_name.as_deref()),
                arg_text(&input.action),
                arg_text(&input.target),
                arg_integer(input.status),
                arg_opt_text(input.source_ip.as_deref()),
                arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql insert audit_log: {e}"))?;

    let id = last_rowid(&qr)?;
    query_one(
        client,
        &format!("SELECT {COLS} FROM audit_logs WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .ok_or_else(|| anyhow::anyhow!("audit_log vanished after append"))?
}

pub async fn list(client: &LibsqlClient, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
    query(
        client,
        &format!("SELECT {COLS} FROM audit_logs ORDER BY id DESC LIMIT ?"),
        &[arg_integer(limit as i64)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}
