use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::{AppState, RateLimitRule};
use gproxy_storage::Scope;
use gproxy_storage::repository::PermissionRepository;
use std::sync::Arc;

async fn resolve_rate_limit_id(
    state: &AppState,
    user_id: i64,
    model_pattern: &str,
) -> Result<i64, HttpError> {
    let rows = state
        .storage()
        .list_user_rate_limits(&gproxy_storage::UserRateLimitQuery {
            user_id: Scope::Eq(user_id),
            ..Default::default()
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    rows.into_iter()
        .find(|row| row.model_pattern == model_pattern)
        .map(|row| row.id)
        .ok_or_else(|| HttpError::not_found("rate limit not found"))
}

/// Response row for rate limits from memory (no timestamps or row id).
#[derive(serde::Serialize)]
pub struct MemoryRateLimitRow {
    pub id: i64,
    pub user_id: i64,
    pub model_pattern: String,
    pub rpm: Option<i32>,
    pub rpd: Option<i32>,
    pub total_tokens: Option<i64>,
}

/// Query filter for rate limits.
#[derive(serde::Deserialize, Default)]
pub struct RateLimitQueryParams {
    pub user_id: Option<Scope<i64>>,
    pub limit: Option<usize>,
}

pub async fn query_rate_limits(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<RateLimitQueryParams>,
) -> Result<Json<Vec<MemoryRateLimitRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let rows = state
        .storage()
        .list_user_rate_limits(&gproxy_storage::UserRateLimitQuery {
            user_id: query.user_id.unwrap_or(Scope::All),
            limit: query.limit.map(|value| value as u64),
            ..Default::default()
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?
        .into_iter()
        .map(|row| MemoryRateLimitRow {
            id: row.id,
            user_id: row.user_id,
            model_pattern: row.model_pattern,
            rpm: row.rpm,
            rpd: row.rpd,
            total_tokens: row.total_tokens,
        })
        .collect();
    Ok(Json(rows))
}

pub async fn upsert_rate_limit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<gproxy_storage::UserRateLimitWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    state
        .storage()
        .upsert_user_rate_limit(payload.clone())
        .await?;

    state.upsert_rate_limit_in_memory(
        payload.user_id,
        RateLimitRule {
            id: payload.id,
            model_pattern: payload.model_pattern.clone(),
            rpm: payload.rpm,
            rpd: payload.rpd,
            total_tokens: payload.total_tokens,
        },
    );
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeleteRateLimitPayload {
    pub user_id: i64,
    pub model_pattern: String,
}

pub async fn delete_rate_limit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteRateLimitPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let id = resolve_rate_limit_id(&state, payload.user_id, &payload.model_pattern).await?;

    state.storage().delete_user_rate_limit(id).await?;

    state.remove_rate_limit_from_memory(payload.user_id, &payload.model_pattern);
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_rate_limits(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<gproxy_storage::UserRateLimitWrite>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for item in items {
        state.storage().upsert_user_rate_limit(item.clone()).await?;
        state.upsert_rate_limit_in_memory(
            item.user_id,
            RateLimitRule {
                id: item.id,
                model_pattern: item.model_pattern.clone(),
                rpm: item.rpm,
                rpd: item.rpd,
                total_tokens: item.total_tokens,
            },
        );
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_rate_limits(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payloads): Json<Vec<DeleteRateLimitPayload>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for p in payloads {
        let id = resolve_rate_limit_id(&state, p.user_id, &p.model_pattern).await?;
        state.storage().delete_user_rate_limit(id).await?;
        state.remove_rate_limit_from_memory(p.user_id, &p.model_pattern);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}
