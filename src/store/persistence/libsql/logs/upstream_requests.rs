//! Upstream-request log ops for the libSQL edge backend (append-only).

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_i64, col_opt_i64, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{UpstreamRequest, UpstreamRequestInput};

const COLS: &str = "id, request_id, at, provider_id, credential_id, url, method, status, \
     latency_ms, headers_json, body, created_at, updated_at, response_body";

fn decode(row: &Row) -> anyhow::Result<UpstreamRequest> {
    Ok(UpstreamRequest {
        id: col_i64(row, 0)?,
        request_id: col_str(row, 1)?,
        at: col_i64(row, 2)?,
        provider_id: col_opt_i64(row, 3)?,
        credential_id: col_opt_i64(row, 4)?,
        url: col_str(row, 5)?,
        method: col_str(row, 6)?,
        status: col_i64(row, 7)?,
        latency_ms: col_i64(row, 8)?,
        headers_json: col_opt_json(row, 9)?,
        body: col_opt_str(row, 10)?,
        created_at: col_i64(row, 11)?,
        updated_at: col_i64(row, 12)?,
        response_body: col_opt_str(row, 13)?,
    })
}

pub async fn append(
    client: &LibsqlClient,
    input: UpstreamRequestInput,
) -> anyhow::Result<UpstreamRequest> {
    let now = now_secs();
    let headers = input
        .headers_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let qr = client
        .execute(
            "INSERT INTO upstream_requests \
             (request_id, at, provider_id, credential_id, url, method, status, latency_ms, \
              headers_json, body, response_body, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                arg_text(&input.request_id),
                arg_integer(input.at),
                arg_opt_i64(input.provider_id),
                arg_opt_i64(input.credential_id),
                arg_text(&input.url),
                arg_text(&input.method),
                arg_integer(input.status),
                arg_integer(input.latency_ms),
                arg_opt_text(headers.as_deref()),
                arg_opt_text(input.body.as_deref()),
                arg_opt_text(input.response_body.as_deref()),
                arg_integer(now),
                arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql insert upstream_request: {e}"))?;

    let id = last_rowid(&qr)?;
    query_one(
        client,
        &format!("SELECT {COLS} FROM upstream_requests WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .ok_or_else(|| anyhow::anyhow!("upstream_request vanished after append"))?
}

pub async fn list(client: &LibsqlClient, request_id: &str) -> anyhow::Result<Vec<UpstreamRequest>> {
    query(
        client,
        &format!("SELECT {COLS} FROM upstream_requests WHERE request_id = ?"),
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
        "UPDATE upstream_requests SET response_body = ?, updated_at = ? WHERE request_id = ?",
        &[
            arg_opt_text(response_body.as_deref()),
            arg_integer(now),
            arg_text(request_id),
        ],
    )
    .await
    .map(|_| ())
}
