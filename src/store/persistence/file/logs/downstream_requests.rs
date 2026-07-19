//! File-backend downstream-request log ops over `downstream_requests.json`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::store::persistence::LogQuery;
use crate::store::persistence::records::{DownstreamRequest, DownstreamRequestInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("downstream_requests.json")
}

/// Remove rows with `created_at < cutoff_ts` (§8-D retention). Returns the count
/// removed; rewrites the file only when something was dropped.
pub(crate) async fn purge_before(root: &Path, cutoff_ts: i64) -> anyhow::Result<u64> {
    let file = path(root);
    let mut t = table::load::<DownstreamRequest>(&file).await?;
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
    input: DownstreamRequestInput,
) -> anyhow::Result<DownstreamRequest> {
    let file = path(root);
    let mut t = table::load::<DownstreamRequest>(&file).await?;
    let now = now_secs();

    let id = t.next_id;
    t.next_id += 1;
    let req = DownstreamRequest {
        id,
        request_id: input.request_id,
        at: input.at,
        method: input.method,
        path: input.path,
        query: input.query,
        status: input.status,
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

pub(crate) async fn list(root: &Path, request_id: &str) -> anyhow::Result<Vec<DownstreamRequest>> {
    Ok(table::load::<DownstreamRequest>(&path(root))
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
    let mut t = table::load::<DownstreamRequest>(&file).await?;
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

/// Filtered rows across all requests, `id` DESC, keyset cursor `before_id`.
pub(crate) async fn query(root: &Path, q: &LogQuery) -> anyhow::Result<Vec<DownstreamRequest>> {
    let mut rows = table::load::<DownstreamRequest>(&path(root)).await?.rows;
    let request_ids = if q.provider_id.is_some() || q.user_id.is_some() || q.route_name.is_some() {
        let usages =
            table::load::<crate::store::persistence::records::Usage>(&root.join("usages.json"))
                .await?;
        Some(
            usages
                .rows
                .into_iter()
                .filter(|u| {
                    q.provider_id.is_none_or(|v| u.provider_id == Some(v))
                        && q.user_id.is_none_or(|v| u.user_id == Some(v))
                        && q.route_name
                            .as_ref()
                            .is_none_or(|v| u.route_name.as_deref() == Some(v.as_str()))
                })
                .map(|u| u.request_id)
                .collect::<HashSet<_>>(),
        )
    } else {
        None
    };

    rows.sort_by_key(|r| std::cmp::Reverse(r.id));
    Ok(rows
        .into_iter()
        .filter(|r| {
            q.before_id.is_none_or(|v| r.id < v)
                && q.at_from.is_none_or(|v| r.at >= v)
                && q.at_to.is_none_or(|v| r.at <= v)
                && request_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&r.request_id))
        })
        .take(q.limit as usize)
        .collect())
}
