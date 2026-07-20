//! Shared read-only admin observability endpoints.
//!
//! Covers usage, usage-rollups, audit, credential health, and request logs.
//! All handlers are GET (read-only, no invalidate), target-independent, and
//! mounted behind `guard_admin`.
//!

use bytes::Bytes;
use http::Method;
use serde::Deserialize;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::{
    AuditLogQuery as StoreAuditLogQuery, LogQuery as StoreLogQuery, UsageQuery as StoreUsageQuery,
};

use super::pagination;
use super::{Request, Resp, internal, parse_i64, query, segments};

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

/// Usage explorer filter + keyset-cursor query.
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
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// `?granularity=hour|day|week|month&from=&to=`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RollupQuery {
    pub granularity: String,
    pub from: i64,
    pub to: i64,
}

/// `?limit=N` for audit listing.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AuditQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub actor_id: Option<i64>,
    pub action: Option<String>,
    pub target: Option<String>,
    pub status: Option<i64>,
    pub source_ip: Option<String>,
    pub limit: Option<u64>,
    pub before_id: Option<String>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// Filters plus `?before_id=&limit=` for the logs listing.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LogsQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub provider_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
    pub page: Option<String>,
    pub page_size: Option<String>,
}

/// Route a read-only observability request to its handler.
///
/// Returns `Some(result)` when the path matches; `None` to fall through.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Request,
    _body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let r = match (&parts.method, segs.as_slice()) {
        // Usage explorer
        (&Method::GET, ["admin", "usage"]) => list_usage(state, parts).await,
        (&Method::GET, ["admin", "usage-summary"]) => usage_summary(state, parts).await,
        (&Method::GET, ["admin", "usage-rollups"]) => list_usage_rollups(state, parts).await,

        // Audit
        (&Method::GET, ["admin", "audit"]) => list_audit(state, parts).await,

        // Credential statuses
        (&Method::GET, ["admin", "credential-statuses"]) => credential_statuses(state, parts).await,
        (&Method::GET, ["admin", "credentials", id, "status"]) => {
            credential_status(state, parts, id).await
        }
        (&Method::GET, ["admin", "credential-model-statuses"]) => {
            credential_model_statuses(state, parts).await
        }
        (&Method::GET, ["admin", "credentials", id, "model-statuses"]) => {
            credential_model_status(state, parts, id).await
        }

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

async fn list_usage(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: UsageFilterQuery = query(parts)?;
    let page = pagination::parse(
        q.page.as_deref(),
        q.page_size.as_deref(),
        q.before_id.is_some(),
    )?;
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
    if let Some(page) = page {
        let result = state
            .persistence
            .query_usages_page(&store_q, &page.store)
            .await
            .map_err(internal)?;
        Resp::json(200, &page.response(result))
    } else {
        let rows = state
            .persistence
            .query_usages(&store_q)
            .await
            .map_err(internal)?;
        Resp::json(200, &rows)
    }
}

async fn usage_summary(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: UsageFilterQuery = query(parts)?;
    let store_q = StoreUsageQuery {
        at_from: q.at_from,
        at_to: q.at_to,
        provider_id: q.provider_id,
        user_id: q.user_id,
        route_name: q.route_name,
        model: q.model,
        before_id: None,
        limit: 0,
    };
    let summary = state
        .persistence
        .summarize_usages(&store_q)
        .await
        .map_err(internal)?;
    Resp::json(200, &summary)
}

async fn list_usage_rollups(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
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

async fn list_audit(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: AuditQuery = query(parts)?;
    let page = pagination::parse(
        q.page.as_deref(),
        q.page_size.as_deref(),
        q.before_id.is_some(),
    )?;
    let store_q = StoreAuditLogQuery {
        at_from: q.at_from,
        at_to: q.at_to,
        actor_id: q.actor_id,
        action: q.action,
        target: q.target,
        status: q.status,
        source_ip: q.source_ip,
    };
    if let Some(page) = page {
        let result = state
            .persistence
            .query_audit_logs_page(&store_q, &page.store)
            .await
            .map_err(internal)?;
        return Resp::json(200, &page.response(result));
    }
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let rows = state
        .persistence
        .list_audit_logs(limit)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn credential_statuses(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let rows = state
        .persistence
        .list_all_credential_statuses()
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn credential_status(state: &AppState, parts: &Request, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let rows = state
        .persistence
        .list_credential_statuses(id)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn credential_model_statuses(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let rows = state
        .persistence
        .list_all_credential_model_statuses()
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn credential_model_status(
    state: &AppState,
    parts: &Request,
    id: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let rows = state
        .persistence
        .list_credential_model_statuses(id)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

async fn list_logs(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: LogsQuery = query(parts)?;
    let page = pagination::parse(
        q.page.as_deref(),
        q.page_size.as_deref(),
        q.before_id.is_some(),
    )?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let store_q = StoreLogQuery {
        at_from: q.at_from,
        at_to: q.at_to,
        provider_id: q.provider_id,
        user_id: q.user_id,
        route_name: q.route_name,
        before_id: q.before_id,
        limit,
    };
    if let Some(page) = page {
        let result = state
            .persistence
            .query_downstream_requests_page(&store_q, &page.store)
            .await
            .map_err(internal)?;
        Resp::json(200, &page.response(result))
    } else {
        let rows = state
            .persistence
            .query_downstream_requests(&store_q)
            .await
            .map_err(internal)?;
        Resp::json(200, &rows)
    }
}

async fn downstream_logs(
    state: &AppState,
    parts: &Request,
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
    parts: &Request,
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

async fn tls_presets(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    Resp::json(200, &crate::api::tls_presets::tls_presets())
}
