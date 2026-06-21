//! Downstream-request log ops for the libSQL edge backend (append-only).

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_i64, col_opt_json, col_opt_str, col_str};
use crate::store::persistence::libsql::util::{
    arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{DownstreamRequest, DownstreamRequestInput};

const COLS: &str = "id, request_id, at, method, path, query, status, headers_json, body, \
     created_at, updated_at, response_body";

fn decode(row: &Row) -> anyhow::Result<DownstreamRequest> {
    Ok(DownstreamRequest {
        id: col_i64(row, 0)?,
        request_id: col_str(row, 1)?,
        at: col_i64(row, 2)?,
        method: col_str(row, 3)?,
        path: col_str(row, 4)?,
        query: col_opt_str(row, 5)?,
        status: col_i64(row, 6)?,
        headers_json: col_opt_json(row, 7)?,
        body: col_opt_str(row, 8)?,
        created_at: col_i64(row, 9)?,
        updated_at: col_i64(row, 10)?,
        response_body: col_opt_str(row, 11)?,
    })
}

pub async fn append(
    client: &LibsqlClient,
    input: DownstreamRequestInput,
) -> anyhow::Result<DownstreamRequest> {
    let now = now_secs();
    let headers = input
        .headers_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let qr = client
        .execute(
            "INSERT INTO downstream_requests \
             (request_id, at, method, path, query, status, headers_json, body, response_body, \
              created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                arg_text(&input.request_id),
                arg_integer(input.at),
                arg_text(&input.method),
                arg_text(&input.path),
                arg_opt_text(input.query.as_deref()),
                arg_integer(input.status),
                arg_opt_text(headers.as_deref()),
                arg_opt_text(input.body.as_deref()),
                arg_opt_text(input.response_body.as_deref()),
                arg_integer(now),
                arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql insert downstream_request: {e}"))?;

    let id = last_rowid(&qr)?;
    query_one(
        client,
        &format!("SELECT {COLS} FROM downstream_requests WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .ok_or_else(|| anyhow::anyhow!("downstream_request vanished after append"))?
}

pub async fn list(
    client: &LibsqlClient,
    request_id: &str,
) -> anyhow::Result<Vec<DownstreamRequest>> {
    query(
        client,
        &format!("SELECT {COLS} FROM downstream_requests WHERE request_id = ?"),
        &[arg_text(request_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

/// Backfill `response_body` (and `updated_at`) on rows matching `request_id`.
/// No-op when no row matches.
pub async fn update_response_body(
    client: &LibsqlClient,
    request_id: &str,
    response_body: Option<String>,
) -> anyhow::Result<()> {
    let now = now_secs();
    exec(
        client,
        "UPDATE downstream_requests SET response_body = ?, updated_at = ? WHERE request_id = ?",
        &[
            arg_opt_text(response_body.as_deref()),
            arg_integer(now),
            arg_text(request_id),
        ],
    )
    .await
    .map(|_| ())
}

/// Recent rows across all requests, `id` DESC, keyset cursor `before_id`.
pub async fn list_recent(
    client: &LibsqlClient,
    limit: u64,
    before_id: Option<i64>,
) -> anyhow::Result<Vec<DownstreamRequest>> {
    let mut sql = format!("SELECT {COLS} FROM downstream_requests WHERE 1=1");
    let mut args: Vec<serde_json::Value> = Vec::new();
    if let Some(v) = before_id {
        sql.push_str(" AND id < ?");
        args.push(arg_integer(v));
    }
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    args.push(arg_integer(limit as i64));
    query(client, &sql, &args)
        .await?
        .iter()
        .map(decode)
        .collect()
}
