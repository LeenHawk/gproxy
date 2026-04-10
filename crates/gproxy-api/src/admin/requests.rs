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

/// Payload for the upstream/downstream `clear` and bulk-delete endpoints.
///
/// `all = true` clears every row matching the (currently empty) filter set
/// regardless of `trace_ids`. `all = false` requires a non-empty
/// `trace_ids` list and only touches those rows. Mirrors the shape used by
/// the sample gproxy admin frontend so the request log UI can issue
/// "clear selected" and "clear all" actions through one route.
#[derive(serde::Deserialize, Default)]
pub struct ClearRequestPayload {
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub trace_ids: Vec<i64>,
}

#[derive(serde::Serialize)]
pub struct ClearRequestAck {
    pub ok: bool,
    pub cleared: u64,
}

fn normalize_trace_ids(raw: Vec<i64>) -> Vec<i64> {
    let mut ids: Vec<i64> = raw.into_iter().filter(|id| *id > 0).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub async fn clear_upstream_request_payloads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<ClearRequestPayload>,
) -> Result<Json<ClearRequestAck>, HttpError> {
    authorize_admin(&headers, &state)?;
    let ids = normalize_trace_ids(payload.trace_ids);
    if !payload.all && ids.is_empty() {
        return Err(HttpError::bad_request(
            "trace_ids must be non-empty when all=false",
        ));
    }
    let cleared = state
        .storage()
        .clear_upstream_request_payloads(if payload.all {
            None
        } else {
            Some(ids.as_slice())
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(Json(ClearRequestAck { ok: true, cleared }))
}

pub async fn clear_downstream_request_payloads(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<ClearRequestPayload>,
) -> Result<Json<ClearRequestAck>, HttpError> {
    authorize_admin(&headers, &state)?;
    let ids = normalize_trace_ids(payload.trace_ids);
    if !payload.all && ids.is_empty() {
        return Err(HttpError::bad_request(
            "trace_ids must be non-empty when all=false",
        ));
    }
    let cleared = state
        .storage()
        .clear_downstream_request_payloads(if payload.all {
            None
        } else {
            Some(ids.as_slice())
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(Json(ClearRequestAck { ok: true, cleared }))
}
