use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::AppState;
use gproxy_storage::*;
use std::sync::Arc;

type UpstreamRequestsParams = UpstreamRequestQuery;
type DownstreamRequestsParams = DownstreamRequestQuery;

pub async fn query_upstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UpstreamRequestsParams>,
) -> Result<Json<Vec<UpstreamRequestQueryRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let rows = state.storage().query_upstream_requests(&query).await?;
    Ok(Json(rows))
}

pub async fn count_upstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UpstreamRequestsParams>,
) -> Result<Json<RequestQueryCount>, HttpError> {
    authorize_admin(&headers, &state)?;
    let count = state.storage().count_upstream_requests(&query).await?;
    Ok(Json(count))
}

#[derive(serde::Deserialize)]
pub struct DeleteRequestsPayload {
    trace_ids: Vec<i64>,
}

pub async fn delete_upstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteRequestsPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state
        .storage()
        .delete_upstream_requests(Some(&payload.trace_ids))
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn query_downstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<DownstreamRequestsParams>,
) -> Result<Json<Vec<DownstreamRequestQueryRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let rows = state.storage().query_downstream_requests(&query).await?;
    Ok(Json(rows))
}

pub async fn count_downstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<DownstreamRequestsParams>,
) -> Result<Json<RequestQueryCount>, HttpError> {
    authorize_admin(&headers, &state)?;
    let count = state.storage().count_downstream_requests(&query).await?;
    Ok(Json(count))
}

pub async fn delete_downstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteRequestsPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state
        .storage()
        .delete_downstream_requests(Some(&payload.trace_ids))
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_upstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state
        .storage()
        .delete_upstream_requests(Some(&ids))
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_downstream_requests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state
        .storage()
        .delete_downstream_requests(Some(&ids))
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(Json(AckResponse { ok: true, id: None }))
}
