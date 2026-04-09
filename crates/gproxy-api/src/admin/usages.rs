use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::AppState;
use gproxy_storage::{UsageQuery, UsageQueryCount, UsageQueryRow, UsageSummary};
use std::sync::Arc;

pub async fn query_usages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UsageQuery>,
) -> Result<Json<Vec<UsageQueryRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let rows = state.storage().query_usages(&query).await?;
    Ok(Json(rows))
}

pub async fn count_usages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UsageQuery>,
) -> Result<Json<UsageQueryCount>, HttpError> {
    authorize_admin(&headers, &state)?;
    let count = state.storage().count_usages(&query).await?;
    Ok(Json(count))
}

/// Aggregate usage rows matching `query` into running totals across the
/// full result set. Powers the admin dashboard's metric cards so the values
/// reflect lifetime usage rather than the visible page slice.
pub async fn summarize_usages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UsageQuery>,
) -> Result<Json<UsageSummary>, HttpError> {
    authorize_admin(&headers, &state)?;
    let summary = state.storage().summarize_usages(&query).await?;
    Ok(Json(summary))
}

pub async fn batch_delete_usages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let trace_ids = if ids.is_empty() {
        None
    } else {
        Some(ids.as_slice())
    };
    state.storage().delete_usages(trace_ids).await?;
    Ok(Json(AckResponse { ok: true, id: None }))
}
