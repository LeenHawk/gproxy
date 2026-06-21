//! Edge admin dispatcher: read-only observability endpoints (B6.2).
//!
//! Covers usage, usage-rollups, audit, credential-statuses, and request logs.
//! All handlers are GET (read-only, no invalidate). The query structs are
//! re-defined cross-target (field-aligned with the native structs in
//! `server/admin/usage.rs`) so they compile on wasm32. Mounted behind
//! `guard_admin`.
//!
//! `GET /admin/credentials/{id}/usage` is intentionally omitted on the edge:
//! it calls `credentials::usage::fetch_usage`, which requires the
//! `upstream-wreq` native feature (unavailable in wasm / edge builds). The
//! route falls through to the caller's 404 — the native axum router handles it.

use bytes::Bytes;
use http::Method;
use http::request::Parts;
use serde::Deserialize;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::UsageQuery as StoreUsageQuery;

use super::{Resp, internal, parse_i64, query, segments};

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

/// Usage explorer filter + keyset-cursor query (re-defined cross-target;
/// field-aligned with `UsageFilterQuery` in `server/admin/usage.rs`).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UsageFilterQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub provider_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
}

/// `?granularity=hour|day|week|month&from=&to=` (re-defined cross-target;
/// field-aligned with `RollupQuery` in `server/admin/usage.rs`).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RollupQuery {
    pub granularity: String,
    pub from: i64,
    pub to: i64,
}

/// `?limit=N` for audit listing (re-defined cross-target; field-aligned with
/// the anonymous `UsageQuery { limit }` struct in `server/admin/usage.rs`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct AuditQuery {
    pub limit: Option<u64>,
}

/// `?before_id=&limit=` for the logs listing (field-aligned with
/// `LogsQuery` in `server/admin/usage.rs`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct LogsQuery {
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
}

/// Route a read-only observability request to its handler.
///
/// Returns `Some(result)` when the path matches; `None` to fall through.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    _body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let r = match (&parts.method, segs.as_slice()) {
        // Usage explorer
        (&Method::GET, ["admin", "usage"]) => list_usage(state, parts).await,
        (&Method::GET, ["admin", "usage-rollups"]) => list_usage_rollups(state, parts).await,

        // Audit
        (&Method::GET, ["admin", "audit"]) => list_audit(state, parts).await,

        // Credential statuses
        (&Method::GET, ["admin", "credential-statuses"]) => credential_statuses(state, parts).await,
        (&Method::GET, ["admin", "credentials", id, "status"]) => {
            credential_status(state, parts, id).await
        }

        // NOTE: GET /admin/credentials/{id}/usage is intentionally absent here.
        // That route calls `credentials::usage::fetch_usage` which requires
        // the native `upstream-wreq` feature. The native axum router handles it.

        // Request logs
        (&Method::GET, ["admin", "logs"]) => list_logs(state, parts).await,
        (&Method::GET, ["admin", "logs", request_id, "downstream"]) => {
            downstream_logs(state, parts, request_id).await
        }
        (&Method::GET, ["admin", "logs", request_id, "upstream"]) => {
            upstream_logs(state, parts, request_id).await
        }

        // Named TLS fingerprint presets for the Console picker (B6 parity).
        (&Method::GET, ["admin", "tls-presets"]) => tls_presets(state, parts).await,

        _ => return None,
    };
    Some(r)
}

// ── handlers ──────────────────────────────────────────────────────────────────

async fn list_usage(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: UsageFilterQuery = query(parts)?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let store_q = StoreUsageQuery {
        at_from: q.at_from,
        at_to: q.at_to,
        provider_id: q.provider_id,
        user_id: q.user_id,
        route_name: q.route_name,
        model: q.model,
        before_id: q.before_id,
        limit,
    };
    let rows = state
        .persistence
        .query_usages(&store_q)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn list_usage_rollups(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: RollupQuery = query(parts)?;
    if !matches!(q.granularity.as_str(), "hour" | "day" | "week" | "month") {
        return Err(ApiError::BadRequest(
            "granularity must be one of hour|day|week|month".into(),
        ));
    }
    let rows = state
        .persistence
        .list_usage_rollups(&q.granularity, q.from, q.to, None)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn list_audit(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: AuditQuery = query(parts)?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let rows = state
        .persistence
        .list_audit_logs(limit)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn credential_statuses(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let rows = state
        .persistence
        .list_all_credential_statuses()
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn credential_status(state: &AppState, parts: &Parts, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let rows = state
        .persistence
        .list_credential_statuses(id)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn list_logs(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: LogsQuery = query(parts)?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let rows = state
        .persistence
        .list_recent_downstream_requests(limit, q.before_id)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn downstream_logs(
    state: &AppState,
    parts: &Parts,
    request_id: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let rows = state
        .persistence
        .list_downstream_requests(request_id)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn upstream_logs(
    state: &AppState,
    parts: &Parts,
    request_id: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let rows = state
        .persistence
        .list_upstream_requests(request_id)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn tls_presets(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    Resp::json(200, &crate::api::tls_presets::tls_presets())
}
