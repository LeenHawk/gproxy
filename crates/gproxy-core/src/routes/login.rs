use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::AppState;

use super::error::HttpError;

#[derive(Debug, Deserialize)]
pub struct LoginPayload {
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user_id: i64,
    pub api_key: String,
    pub generated: bool,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginPayload>,
) -> Result<Json<LoginResponse>, HttpError> {
    let users = state.load_users();
    let user = gproxy_admin::authenticate_user_password(
        &users,
        payload.name.as_str(),
        payload.password.as_str(),
    )?;

    let keys = state.load_keys();
    if let Some(existing) = keys
        .values()
        .filter(|row| row.user_id == user.id && row.enabled)
        .min_by_key(|row| row.id)
    {
        return Ok(Json(LoginResponse {
            user_id: user.id,
            api_key: existing.api_key.clone(),
            generated: false,
        }));
    }

    let mut id = None;
    let mut api_key = None;
    for _ in 0..8 {
        let candidate =
            gproxy_admin::generate_unique_user_api_key(&keys).map_err(HttpError::from)?;
        let create_result = state
            .load_storage()
            .create_user_key(user.id, candidate.as_str(), Some("auto-generated"), true)
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
        user_id: user.id,
        api_key: api_key.clone(),
        label: Some("auto-generated".to_string()),
        enabled: true,
    };
    state.upsert_user_key_in_memory(write);

    Ok(Json(LoginResponse {
        user_id: user.id,
        api_key,
        generated: true,
    }))
}
