use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, State};
use serde::Serialize;

use gproxy_server::AppState;

use crate::auth::SessionUser;
use crate::error::HttpError;

#[derive(Serialize)]
pub struct UserKeyRow {
    pub api_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}

/// List the authenticated user's API keys (from memory).
pub async fn query_keys(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionUser>,
) -> Result<Json<Vec<UserKeyRow>>, HttpError> {
    let user_id = session.user_id;
    let keys: Vec<UserKeyRow> = state
        .keys_for_user(user_id)
        .into_iter()
        .map(|k| UserKeyRow {
            api_key: k.api_key,
            label: k.label,
            enabled: k.enabled,
        })
        .collect();
    Ok(Json(keys))
}

#[derive(serde::Deserialize)]
pub struct GenerateKeyPayload {
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Serialize)]
pub struct GenerateKeyResponse {
    pub ok: bool,
    pub api_key: String,
}

/// User-facing key generation — generates a new API key for the authenticated user.
pub async fn generate_key(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionUser>,
    Json(payload): Json<GenerateKeyPayload>,
) -> Result<Json<GenerateKeyResponse>, HttpError> {
    let user_id = session.user_id;
    let api_key = crate::admin::users::generate_unique_api_key_for(&state);
    let id = state
        .storage()
        .create_user_key(user_id, &api_key, payload.label.as_deref(), true)
        .await?;
    state.upsert_key_in_memory(gproxy_server::MemoryUserKey {
        id,
        user_id,
        api_key: api_key.clone(),
        label: payload.label.clone(),
        enabled: true,
    });
    Ok(Json(GenerateKeyResponse { ok: true, api_key }))
}
