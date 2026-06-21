//! Read-only admin observability endpoints (§15, native-only): usage rows,
//! usage rollups, persisted credential health, and the request logs. Plus one
//! LIVE endpoint — `credential_usage` queries the upstream usage API on demand.
//!
//! The read-only handlers serialize their records directly — none of
//! `Usage` / `UsageRollup` / `CredentialStatus` / `DownstreamRequest` /
//! `UpstreamRequest` carries a secret. The log records' `headers_json` / `body`
//! are §14.3-redacted at capture, so reading them back is safe. Mounted behind
//! `require_admin`.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use super::crud::internal;
use crate::api::error::ApiError;
use crate::api::tls_presets::TlsPreset;
use crate::app::AppState;
use crate::channel::UsageSnapshot;
use crate::store::persistence::UsageQuery as StoreUsageQuery;
use crate::store::persistence::records::{
    AuditLog, CredentialStatus, DownstreamRequest, UpstreamRequest, Usage, UsageRollup,
};

/// `?limit=N` for the audit listing; defaults to 100, capped at 1000.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct UsageQuery {
    pub limit: Option<u64>,
}

const DEFAULT_USAGE_LIMIT: u64 = 100;
const MAX_USAGE_LIMIT: u64 = 1000;

/// The usage-explorer selector (B4): optional filters + `before_id` keyset
/// cursor + `limit` (default 100, capped 1000). All filter params are optional;
/// omitting them yields the recency-only listing.
#[derive(Debug, Clone, Deserialize)]
pub struct UsageFilterQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub provider_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
}

/// `GET /admin/usage` — filtered + keyset-paginated usage rows (id desc). With
/// no params this is the recency-only listing; `before_id` pages backwards.
pub async fn list_usage(
    State(state): State<AppState>,
    Query(q): Query<UsageFilterQuery>,
) -> Result<Json<Vec<Usage>>, ApiError> {
    let limit = q.limit.unwrap_or(DEFAULT_USAGE_LIMIT).min(MAX_USAGE_LIMIT);
    let query = StoreUsageQuery {
        at_from: q.at_from,
        at_to: q.at_to,
        provider_id: q.provider_id,
        user_id: q.user_id,
        route_name: q.route_name,
        model: q.model,
        before_id: q.before_id,
        limit,
    };
    Ok(Json(
        state
            .persistence
            .query_usages(&query)
            .await
            .map_err(internal)?,
    ))
}

/// The `?granularity=&from=&to=` selector for rollups.
#[derive(Debug, Clone, Deserialize)]
pub struct RollupQuery {
    pub granularity: String,
    pub from: i64,
    pub to: i64,
}

/// `GET /admin/usage-rollups?granularity=hour|day|week|month&from=&to=` —
/// rollup buckets for one granularity in `[from, to]`.
pub async fn list_usage_rollups(
    State(state): State<AppState>,
    Query(q): Query<RollupQuery>,
) -> Result<Json<Vec<UsageRollup>>, ApiError> {
    if !matches!(q.granularity.as_str(), "hour" | "day" | "week" | "month") {
        return Err(ApiError::BadRequest(
            "granularity must be one of hour|day|week|month".into(),
        ));
    }
    Ok(Json(
        state
            .persistence
            .list_usage_rollups(&q.granularity, q.from, q.to, None)
            .await
            .map_err(internal)?,
    ))
}

/// `GET /admin/audit?limit=N` — the most recent audit rows (id desc). Audit
/// rows carry only method/path/actor/status/ip; never a secret.
pub async fn list_audit(
    State(state): State<AppState>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<Vec<AuditLog>>, ApiError> {
    let limit = q.limit.unwrap_or(DEFAULT_USAGE_LIMIT).min(MAX_USAGE_LIMIT);
    Ok(Json(
        state
            .persistence
            .list_audit_logs(limit)
            .await
            .map_err(internal)?,
    ))
}

/// `GET /admin/credentials/{id}/status` — the persisted credential health
/// snapshots (§16.3 edge-triggered breaker/cooldown rows) for one credential.
pub async fn credential_status(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<CredentialStatus>>, ApiError> {
    Ok(Json(
        state
            .persistence
            .list_credential_statuses(id)
            .await
            .map_err(internal)?,
    ))
}

/// `GET /admin/credential-statuses` — all persisted credential health snapshots
/// (B5 batch endpoint). Callers group by `credential_id`; at most one row per
/// `(credential_id, channel)` pair is stored.
pub async fn credential_statuses(
    State(state): State<AppState>,
) -> Result<Json<Vec<CredentialStatus>>, ApiError> {
    Ok(Json(
        state
            .persistence
            .list_all_credential_statuses()
            .await
            .map_err(internal)?,
    ))
}

/// `GET /admin/credentials/{id}/usage` — LIVE per-credential upstream usage /
/// quota for OAuth subscription channels. Resolves the credential's client,
/// refreshes the token if stale, and queries the provider's usage endpoint on
/// demand. NOT cached — call sparingly (some upstreams rate-limit it). Channels
/// without a usage endpoint (api-key / vertex) return 400.
pub async fn credential_usage(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<UsageSnapshot>, ApiError> {
    crate::credentials::usage::fetch_usage(&state, id)
        .await
        .map(Json)
        .map_err(usage_error)
}

/// Map a [`UsageError`](crate::credentials::usage::UsageError) onto the admin
/// API status set: missing entities → 404, no-endpoint / bad credential → 400,
/// transport / decrypt / non-2xx upstream → 500.
fn usage_error(e: crate::credentials::usage::UsageError) -> ApiError {
    use crate::credentials::usage::UsageError as U;
    match e {
        U::CredentialNotFound | U::ProviderNotFound | U::UnknownChannel(_) => {
            ApiError::NotFound(e.to_string())
        }
        U::Unsupported | U::Channel(_) => ApiError::BadRequest(e.to_string()),
        U::Decrypt(_) | U::Upstream(_) | U::Status(_) => ApiError::Internal(e.to_string()),
    }
}

/// `GET /admin/providers/{id}/upstream-models` — LIVE: pull the provider's model
/// list from its upstream (picks an enabled credential, refreshes token, sends a
/// `list_models` request). Native-only (needs the upstream transport). NOT
/// cached. No enabled credential → 400.
pub async fn upstream_models(
    State(state): State<AppState>,
    Path(provider_id): Path<i64>,
) -> Result<Json<Vec<crate::credentials::upstream_models::UpstreamModel>>, ApiError> {
    crate::credentials::upstream_models::fetch_models(&state, provider_id)
        .await
        .map(Json)
        .map_err(models_error)
}

fn models_error(e: crate::credentials::upstream_models::ModelsError) -> ApiError {
    use crate::credentials::upstream_models::ModelsError as M;
    match e {
        M::ProviderNotFound | M::UnknownChannel(_) => ApiError::NotFound(e.to_string()),
        M::NoCredential | M::Channel(_) | M::Status(_) => ApiError::BadRequest(e.to_string()),
        M::Decrypt(_) | M::Upstream(_) | M::Internal(_) => ApiError::Internal(e.to_string()),
    }
}

/// `GET /admin/logs` — recent downstream request logs (id desc, `before_id`
/// keyset, `limit` default 100 capped 1000). The logs explorer listing.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
}

pub async fn list_logs(
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<Vec<DownstreamRequest>>, ApiError> {
    let limit = q.limit.unwrap_or(DEFAULT_USAGE_LIMIT).min(MAX_USAGE_LIMIT);
    Ok(Json(
        state
            .persistence
            .list_recent_downstream_requests(limit, q.before_id)
            .await
            .map_err(internal)?,
    ))
}

/// `GET /admin/logs/{request_id}/downstream` — downstream (client → proxy) log
/// entries correlated by `request_id` (§15).
pub async fn downstream_logs(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> Result<Json<Vec<DownstreamRequest>>, ApiError> {
    Ok(Json(
        state
            .persistence
            .list_downstream_requests(&request_id)
            .await
            .map_err(internal)?,
    ))
}

/// `GET /admin/logs/{request_id}/upstream` — upstream (proxy → provider) log
/// entries correlated by `request_id` (§15).
pub async fn upstream_logs(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> Result<Json<Vec<UpstreamRequest>>, ApiError> {
    Ok(Json(
        state
            .persistence
            .list_upstream_requests(&request_id)
            .await
            .map_err(internal)?,
    ))
}

/// `GET /admin/tls-presets` — static curated list of named TLS fingerprint
/// presets (agent profiles from `docs/agent-tls-fingerprints.md` §5).
/// No DB access — returns a constant list. Use the Console picker to store a
/// chosen preset's `fingerprint` blob into a provider/credential `tls_fingerprint`.
pub async fn tls_presets() -> Json<Vec<TlsPreset>> {
    Json(crate::api::tls_presets::tls_presets())
}
