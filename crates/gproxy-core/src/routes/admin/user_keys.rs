use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;

use crate::AppState;

use super::{Ack, DeleteById, HttpError, authorize_admin};

#[derive(Debug, Deserialize)]
pub(super) struct GenerateUserKeyPayload {
    user_id: i64,
    #[serde(default)]
    label: Option<String>,
}

pub(super) async fn query_user_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<gproxy_storage::UserKeyQuery>,
) -> Result<Json<Vec<gproxy_storage::UserKeyQueryRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let keys = state.load_keys();
    Ok(Json(gproxy_admin::query_user_keys(&keys, query).await?))
}

pub(super) async fn generate_user_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<GenerateUserKeyPayload>,
) -> Result<Json<gproxy_storage::UserKeyWrite>, HttpError> {
    authorize_admin(&headers, &state)?;
    let users = state.load_users();
    if !users.iter().any(|row| row.id == payload.user_id) {
        return Err(HttpError::from(gproxy_admin::AdminApiError::InvalidInput(
            format!("user {} not found", payload.user_id),
        )));
    }
    let keys = state.load_keys();
    let mut id = None;
    let mut api_key = None;
    for _ in 0..8 {
        let candidate = gproxy_admin::generate_unique_user_api_key(&keys)?;
        let create_result = state
            .load_storage()
            .create_user_key(
                payload.user_id,
                candidate.as_str(),
                payload.label.as_deref(),
                true,
            )
            .await;
        match create_result {
            Ok(created_id) => {
                id = Some(created_id);
                api_key = Some(candidate);
                break;
            }
            Err(err) => {
                let message = err.to_string().to_ascii_lowercase();
                if !message.contains("unique") {
                    return Err(HttpError::from(gproxy_admin::AdminApiError::from(err)));
                }
            }
        }
    }
    let id = id.ok_or_else(|| {
        HttpError::from(gproxy_admin::AdminApiError::InvalidInput(
            "failed to generate unique user key".to_string(),
        ))
    })?;
    let api_key = api_key.ok_or_else(|| {
        HttpError::from(gproxy_admin::AdminApiError::InvalidInput(
            "failed to generate unique user key".to_string(),
        ))
    })?;
    let write = gproxy_storage::UserKeyWrite {
        id,
        user_id: payload.user_id,
        api_key,
        label: payload.label.filter(|v| !v.trim().is_empty()),
        enabled: true,
    };
    state.upsert_user_key_in_memory(write.clone());
    Ok(Json(write))
}

pub(super) async fn delete_user_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteById>,
) -> Result<Json<Ack>, HttpError> {
    authorize_admin(&headers, &state)?;
    state.delete_user_key_in_memory(payload.id);
    gproxy_admin::delete_user_key(&state.storage_writes, payload.id).await?;
    Ok(Json(Ack { ok: true }))
}
