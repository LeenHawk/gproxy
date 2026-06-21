//! File-backend upstream-request log ops over `upstream_requests.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{UpstreamRequest, UpstreamRequestInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("upstream_requests.json")
}

/// Remove rows with `created_at < cutoff_ts` (§8-D retention). Returns the count
/// removed; rewrites the file only when something was dropped.
pub(crate) async fn purge_before(root: &Path, cutoff_ts: i64) -> anyhow::Result<u64> {
    let file = path(root);
    let mut t = table::load::<UpstreamRequest>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.created_at >= cutoff_ts);
    let removed = (before - t.rows.len()) as u64;
    if removed > 0 {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn append(
    root: &Path,
    input: UpstreamRequestInput,
) -> anyhow::Result<UpstreamRequest> {
    let file = path(root);
    let mut t = table::load::<UpstreamRequest>(&file).await?;
    let now = now_secs();

    let id = t.next_id;
    t.next_id += 1;
    let req = UpstreamRequest {
        id,
        request_id: input.request_id,
        at: input.at,
        provider_id: input.provider_id,
        credential_id: input.credential_id,
        url: input.url,
        method: input.method,
        status: input.status,
        latency_ms: input.latency_ms,
        headers_json: input.headers_json,
        body: input.body,
        response_body: input.response_body,
        created_at: now,
        updated_at: now,
    };
    t.rows.push(req.clone());

    table::store(&file, &t).await?;
    Ok(req)
}

pub(crate) async fn list(root: &Path, request_id: &str) -> anyhow::Result<Vec<UpstreamRequest>> {
    Ok(table::load::<UpstreamRequest>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|r| r.request_id == request_id)
        .collect())
}

/// Backfill `response_body` (and `updated_at`) on rows matching `request_id`.
/// No-op when no row matches. Caller holds the backend write lock.
pub(crate) async fn update_response_body(
    root: &Path,
    request_id: &str,
    response_body: Option<String>,
) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<UpstreamRequest>(&file).await?;
    let now = now_secs();
    let mut changed = false;
    for r in t.rows.iter_mut().filter(|r| r.request_id == request_id) {
        r.response_body = response_body.clone();
        r.updated_at = now;
        changed = true;
    }
    if changed {
        table::store(&file, &t).await?;
    }
    Ok(())
}
