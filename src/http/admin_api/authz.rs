//! Edge admin dispatcher: authz-scoped entities (§8-C, B6.2).
//!
//! Covers `route-permissions`, `rate-limits`, and `quotas`, each keyed by
//! `(scope, scope_id)`. The query struct and dispatch logic mirror the native
//! `server/admin/crud/authz.rs` handlers, but target the cross-target [`Resp`]
//! / [`ApiError`] API (no axum extractors). This file is compiled for both the
//! wasm edge worker and native `cfg(test)` builds.

use bytes::Bytes;
use http::Method;
use http::request::Parts;
use serde::Deserialize;

use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::{
    QuotaInput, RateLimit, RateLimitInput, RoutePermission, RoutePermissionInput, Scope,
};

use super::{Resp, internal, json_body, parse_i64, query, segments};

/// `?scope=org&scope_id=1` query params (re-defined cross-target; field-aligned
/// with the native `ScopeQuery` in `server/admin/crud/authz.rs`).
#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct ScopeQuery {
    pub scope: Scope,
    pub scope_id: i64,
}

/// Route an authz-scoped request to its handler.
///
/// Returns `Some(result)` when the path matches one of the nine authz routes;
/// `None` to fall through to the next sub-dispatcher.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let r = match (&parts.method, segs.as_slice()) {
        // ── route-permissions ─────────────────────────────────────────────────
        (&Method::GET, ["admin", "route-permissions"]) => {
            list_route_permissions(state, parts).await
        }
        (&Method::POST, ["admin", "route-permissions"]) => {
            upsert_route_permission(state, parts, body).await
        }
        (&Method::DELETE, ["admin", "route-permissions", id]) => {
            delete_route_permission(state, parts, id).await
        }

        // ── rate-limits ───────────────────────────────────────────────────────
        (&Method::GET, ["admin", "rate-limits"]) => list_rate_limits(state, parts).await,
        (&Method::POST, ["admin", "rate-limits"]) => upsert_rate_limit(state, parts, body).await,
        (&Method::DELETE, ["admin", "rate-limits", id]) => {
            delete_rate_limit(state, parts, id).await
        }

        // ── quotas ────────────────────────────────────────────────────────────
        (&Method::GET, ["admin", "quotas"]) => get_quota(state, parts).await,
        (&Method::POST, ["admin", "quotas"]) => upsert_quota(state, parts, body).await,
        (&Method::DELETE, ["admin", "quotas", id]) => delete_quota(state, parts, id).await,

        _ => return None,
    };
    Some(r)
}

// ── route-permissions ─────────────────────────────────────────────────────────

async fn list_route_permissions(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: ScopeQuery = query(parts)?;
    let recs: Vec<RoutePermission> = state
        .persistence
        .list_route_permissions(q.scope, q.scope_id)
        .await
        .map_err(internal)?;
    Resp::json(200, &recs)
}

async fn upsert_route_permission(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let input: RoutePermissionInput = json_body(body)?;
    let rec = state
        .persistence
        .upsert_route_permission(input)
        .await
        .map_err(ApiError::from_upsert)?;
    invalidate(state).await;
    Resp::json(200, &rec)
}

async fn delete_route_permission(
    state: &AppState,
    parts: &Parts,
    id: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    if state
        .persistence
        .delete_route_permission(id)
        .await
        .map_err(internal)?
    {
        invalidate(state).await;
        Ok(Resp::no_content())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}

// ── rate-limits ───────────────────────────────────────────────────────────────

async fn list_rate_limits(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: ScopeQuery = query(parts)?;
    let recs: Vec<RateLimit> = state
        .persistence
        .list_rate_limits(q.scope, q.scope_id)
        .await
        .map_err(internal)?;
    Resp::json(200, &recs)
}

async fn upsert_rate_limit(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let input: RateLimitInput = json_body(body)?;
    let rec = state
        .persistence
        .upsert_rate_limit(input)
        .await
        .map_err(ApiError::from_upsert)?;
    invalidate(state).await;
    Resp::json(200, &rec)
}

async fn delete_rate_limit(state: &AppState, parts: &Parts, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    if state
        .persistence
        .delete_rate_limit(id)
        .await
        .map_err(internal)?
    {
        invalidate(state).await;
        Ok(Resp::no_content())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}

// ── quotas ────────────────────────────────────────────────────────────────────

async fn get_quota(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let q: ScopeQuery = query(parts)?;
    match state
        .persistence
        .get_quota(q.scope, q.scope_id)
        .await
        .map_err(internal)?
    {
        Some(quota) => Resp::json(200, &quota),
        None => Err(ApiError::NotFound("not found".into())),
    }
}

async fn upsert_quota(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let input: QuotaInput = json_body(body)?;
    let rec = state
        .persistence
        .upsert_quota(input)
        .await
        .map_err(ApiError::from_upsert)?;
    invalidate(state).await;
    Resp::json(200, &rec)
}

async fn delete_quota(state: &AppState, parts: &Parts, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    if state.persistence.delete_quota(id).await.map_err(internal)? {
        invalidate(state).await;
        Ok(Resp::no_content())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}
