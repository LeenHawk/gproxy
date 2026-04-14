use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, State};
use serde::Serialize;

use gproxy_server::AppState;
use gproxy_storage::UserRepository;

use crate::auth::SessionUser;
use crate::error::HttpError;

#[derive(Serialize)]
pub struct UserKeyRow {
    pub id: i64,
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
            id: k.id,
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
    pub id: i64,
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
    Ok(Json(GenerateKeyResponse {
        ok: true,
        id,
        api_key,
    }))
}

#[derive(serde::Deserialize)]
pub struct DeleteKeyPayload {
    pub id: i64,
}

#[derive(serde::Deserialize)]
pub struct UpdateKeyEnabledPayload {
    pub id: i64,
    pub enabled: bool,
}

pub async fn update_key_enabled(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionUser>,
    Json(payload): Json<UpdateKeyEnabledPayload>,
) -> Result<Json<crate::error::AckResponse>, HttpError> {
    let user_id = session.user_id;
    let key = state
        .keys_for_user(user_id)
        .into_iter()
        .find(|key| key.id == payload.id)
        .ok_or_else(|| HttpError::not_found("user key not found"))?;
    state
        .storage()
        .upsert_user_key(gproxy_storage::UserKeyWrite {
            id: key.id,
            user_id: key.user_id,
            api_key: key.api_key.clone(),
            label: key.label.clone(),
            enabled: payload.enabled,
        })
        .await?;
    state.upsert_key_in_memory(gproxy_server::MemoryUserKey {
        enabled: payload.enabled,
        ..key
    });
    Ok(Json(crate::error::AckResponse { ok: true, id: None }))
}

pub async fn delete_key(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionUser>,
    Json(payload): Json<DeleteKeyPayload>,
) -> Result<Json<crate::error::AckResponse>, HttpError> {
    let user_id = session.user_id;
    let owned = state
        .keys_for_user(user_id)
        .into_iter()
        .any(|key| key.id == payload.id);
    if !owned {
        return Err(HttpError::not_found("user key not found"));
    }
    state.storage().delete_user_key(payload.id).await?;
    state.remove_key_from_memory(payload.id);
    Ok(Json(crate::error::AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_keys(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionUser>,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<crate::error::AckResponse>, HttpError> {
    let user_id = session.user_id;
    let owned: std::collections::HashSet<i64> = state
        .keys_for_user(user_id)
        .into_iter()
        .map(|k| k.id)
        .collect();
    for id in &ids {
        if !owned.contains(id) {
            return Err(HttpError::not_found("user key not found"));
        }
    }
    for id in &ids {
        state.storage().delete_user_key(*id).await?;
        state.remove_key_from_memory(*id);
    }
    Ok(Json(crate::error::AckResponse { ok: true, id: None }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{Json, extract::Extension};

    use super::{UpdateKeyEnabledPayload, update_key_enabled};
    use crate::auth::SessionUser;
    use gproxy_server::{AppState, AppStateBuilder, GlobalConfig};
    use gproxy_storage::{SeaOrmStorage, repository::UserRepository};

    async fn build_test_state() -> Arc<AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 2,
                name: "alice".to_string(),
                password: crate::login::hash_password("user-password"),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 20,
                user_id: 2,
                api_key: "sk-user".to_string(),
                label: Some("user".to_string()),
                enabled: true,
            })
            .await
            .expect("seed user key");

        let state = Arc::new(
            AppStateBuilder::new()
                .engine(gproxy_sdk::engine::engine::GproxyEngine::builder().build())
                .storage(storage)
                .config(GlobalConfig {
                    dsn: "sqlite::memory:".to_string(),
                    ..GlobalConfig::default()
                })
                .build(),
        );
        crate::bootstrap::reload_from_db(&state)
            .await
            .expect("reload state");
        state
    }

    #[tokio::test]
    async fn update_key_enabled_updates_owned_key() {
        let state = build_test_state().await;

        let _ = update_key_enabled(
            axum::extract::State(state.clone()),
            Extension(SessionUser {
                user_id: 2,
                is_admin: false,
            }),
            Json(UpdateKeyEnabledPayload {
                id: 20,
                enabled: false,
            }),
        )
        .await
        .expect("update key enabled");

        let key = state
            .keys_for_user(2)
            .into_iter()
            .find(|key| key.id == 20)
            .expect("updated key");
        assert!(!key.enabled);
    }
}
