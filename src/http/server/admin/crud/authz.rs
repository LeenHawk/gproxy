//! CRUD handlers for the authz scoped entities (§8-C): `route_permissions` and
//! `rate_limits` (keyed by `(scope, scope_id)`, listed by that key) and
//! `quotas` (unique per `(scope, scope_id)`, fetched as an `Option`).
//!
//! These records carry no secrets, so they serialize directly (no redacting
//! view). Every upsert and delete invalidates the local snapshot + broadcasts.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use super::{internal, upsert_err};
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::{
    Quota, QuotaInput, RateLimit, RateLimitInput, RoutePermission, RoutePermissionInput, Scope,
};

/// The `?scope=org&scope_id=1` selector shared by the list (and quota get)
/// handlers. `Scope` deserializes snake_case ("org"/"team"/"user").
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ScopeQuery {
    pub scope: Scope,
    pub scope_id: i64,
}

/// `GET /admin/route-permissions?scope=org&scope_id=1` — list for one scope.
pub async fn list_route_permissions(
    State(state): State<AppState>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<Vec<RoutePermission>>, ApiError> {
    Ok(Json(
        state
            .persistence
            .list_route_permissions(q.scope, q.scope_id)
            .await
            .map_err(internal)?,
    ))
}

/// `POST /admin/route-permissions` — upsert, then invalidate.
pub async fn upsert_route_permission(
    State(state): State<AppState>,
    Json(input): Json<RoutePermissionInput>,
) -> Result<Json<RoutePermission>, ApiError> {
    let rec = state
        .persistence
        .upsert_route_permission(input)
        .await
        .map_err(upsert_err)?;
    invalidate(&state).await;
    Ok(Json(rec))
}

/// `DELETE /admin/route-permissions/{id}` — 204 on removal, 404 otherwise.
pub async fn delete_route_permission(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<axum::response::Response, ApiError> {
    if state
        .persistence
        .delete_route_permission(id)
        .await
        .map_err(internal)?
    {
        invalidate(&state).await;
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}

/// `GET /admin/rate-limits?scope=org&scope_id=1` — list for one scope.
pub async fn list_rate_limits(
    State(state): State<AppState>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<Vec<RateLimit>>, ApiError> {
    Ok(Json(
        state
            .persistence
            .list_rate_limits(q.scope, q.scope_id)
            .await
            .map_err(internal)?,
    ))
}

/// `POST /admin/rate-limits` — upsert, then invalidate.
pub async fn upsert_rate_limit(
    State(state): State<AppState>,
    Json(input): Json<RateLimitInput>,
) -> Result<Json<RateLimit>, ApiError> {
    let rec = state
        .persistence
        .upsert_rate_limit(input)
        .await
        .map_err(upsert_err)?;
    invalidate(&state).await;
    Ok(Json(rec))
}

/// `DELETE /admin/rate-limits/{id}` — 204 on removal, 404 otherwise.
pub async fn delete_rate_limit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<axum::response::Response, ApiError> {
    if state
        .persistence
        .delete_rate_limit(id)
        .await
        .map_err(internal)?
    {
        invalidate(&state).await;
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}

/// `GET /admin/quotas?scope=org&scope_id=1` — the quota for that scope, or 404
/// when none is set.
pub async fn get_quota(
    State(state): State<AppState>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<Quota>, ApiError> {
    match state
        .persistence
        .get_quota(q.scope, q.scope_id)
        .await
        .map_err(internal)?
    {
        Some(quota) => Ok(Json(quota)),
        None => Err(ApiError::NotFound("not found".into())),
    }
}

/// `POST /admin/quotas` — upsert (unique per scope), then invalidate.
pub async fn upsert_quota(
    State(state): State<AppState>,
    Json(input): Json<QuotaInput>,
) -> Result<Json<Quota>, ApiError> {
    let rec = state
        .persistence
        .upsert_quota(input)
        .await
        .map_err(upsert_err)?;
    invalidate(&state).await;
    Ok(Json(rec))
}

/// `DELETE /admin/quotas/{id}` — 204 on removal, 404 otherwise.
pub async fn delete_quota(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<axum::response::Response, ApiError> {
    if state.persistence.delete_quota(id).await.map_err(internal)? {
        invalidate(&state).await;
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}
