//! Usage ops for the libSQL edge backend (append-only, idempotent by `request_id`).

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::UsageQuery;
use crate::store::persistence::libsql::row::{
    Row, col_decimal, col_i64, col_opt_i64, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_opt_i64, arg_opt_text, last_rowid, now_secs, query as run_query, query_one,
};
use crate::store::persistence::records::{Usage, UsageInput};
use serde_json::Value;

const COLS: &str = "id, request_id, at, route_name, provider_id, credential_id, org_id, team_id, \
     user_id, user_key_id, operation, kind, model, input_tokens, output_tokens, \
     cache_read_tokens, cache_creation_5m_tokens, cache_creation_1h_tokens, cost, latency_ms, \
     usage_source, ended, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Usage> {
    Ok(Usage {
        id: col_i64(row, 0)?,
        request_id: col_str(row, 1)?,
        at: col_i64(row, 2)?,
        route_name: col_opt_str(row, 3)?,
        provider_id: col_opt_i64(row, 4)?,
        credential_id: col_opt_i64(row, 5)?,
        org_id: col_opt_i64(row, 6)?,
        team_id: col_opt_i64(row, 7)?,
        user_id: col_opt_i64(row, 8)?,
        user_key_id: col_opt_i64(row, 9)?,
        operation: col_str(row, 10)?,
        kind: col_str(row, 11)?,
        model: col_opt_str(row, 12)?,
        input_tokens: col_i64(row, 13)?,
        output_tokens: col_i64(row, 14)?,
        cache_read_tokens: col_i64(row, 15)?,
        cache_creation_5m_tokens: col_i64(row, 16)?,
        cache_creation_1h_tokens: col_i64(row, 17)?,
        cost: col_decimal(row, 18)?,
        latency_ms: col_i64(row, 19)?,
        usage_source: col_str(row, 20)?,
        ended: col_str(row, 21)?,
        created_at: col_i64(row, 22)?,
        updated_at: col_i64(row, 23)?,
    })
}

/// Append a usage row; `Ok(None)` when a row with the same `request_id` already
/// exists (idempotent settle, §17).
pub async fn append(client: &LibsqlClient, input: UsageInput) -> anyhow::Result<Option<Usage>> {
    if query_one(
        client,
        "SELECT id FROM usages WHERE request_id = ?",
        &[arg_text(&input.request_id)],
    )
    .await?
    .is_some()
    {
        return Ok(None);
    }

    let now = now_secs();
    let qr = client
        .execute(
            "INSERT INTO usages \
             (request_id, at, route_name, provider_id, credential_id, org_id, team_id, user_id, \
              user_key_id, operation, kind, model, input_tokens, output_tokens, \
              cache_read_tokens, cache_creation_5m_tokens, cache_creation_1h_tokens, cost, \
              latency_ms, usage_source, ended, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                arg_text(&input.request_id),
                arg_integer(input.at),
                arg_opt_text(input.route_name.as_deref()),
                arg_opt_i64(input.provider_id),
                arg_opt_i64(input.credential_id),
                arg_opt_i64(input.org_id),
                arg_opt_i64(input.team_id),
                arg_opt_i64(input.user_id),
                arg_opt_i64(input.user_key_id),
                arg_text(&input.operation),
                arg_text(&input.kind),
                arg_opt_text(input.model.as_deref()),
                arg_integer(input.input_tokens),
                arg_integer(input.output_tokens),
                arg_integer(input.cache_read_tokens),
                arg_integer(input.cache_creation_5m_tokens),
                arg_integer(input.cache_creation_1h_tokens),
                arg_text(&input.cost.to_string()),
                arg_integer(input.latency_ms),
                arg_text(&input.usage_source),
                arg_text(&input.ended),
                arg_integer(now),
                arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql insert usage: {e}"))?;

    let id = last_rowid(&qr)?;
    query_one(
        client,
        &format!("SELECT {COLS} FROM usages WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, limit: u64) -> anyhow::Result<Vec<Usage>> {
    run_query(
        client,
        &format!("SELECT {COLS} FROM usages ORDER BY id DESC LIMIT ?"),
        &[arg_integer(limit as i64)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

/// Filtered + keyset-paginated usage rows (B4). Builds a dynamic WHERE with
/// bound args (only the supplied filters become predicates), ordered `id` DESC.
pub async fn query(client: &LibsqlClient, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
    let mut sql = format!("SELECT {COLS} FROM usages WHERE 1=1");
    let mut args: Vec<Value> = Vec::new();
    if let Some(v) = q.at_from {
        sql.push_str(" AND at >= ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.at_to {
        sql.push_str(" AND at <= ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.provider_id {
        sql.push_str(" AND provider_id = ?");
        args.push(arg_opt_i64(Some(v)));
    }
    if let Some(v) = q.user_id {
        sql.push_str(" AND user_id = ?");
        args.push(arg_opt_i64(Some(v)));
    }
    if let Some(ref v) = q.route_name {
        sql.push_str(" AND route_name = ?");
        args.push(arg_opt_text(Some(v.as_str())));
    }
    if let Some(ref v) = q.model {
        sql.push_str(" AND model = ?");
        args.push(arg_opt_text(Some(v.as_str())));
    }
    if let Some(v) = q.before_id {
        sql.push_str(" AND id < ?");
        args.push(arg_integer(v));
    }
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    args.push(arg_integer(q.limit as i64));

    run_query(client, &sql, &args)
        .await?
        .iter()
        .map(decode)
        .collect()
}
