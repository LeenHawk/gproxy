use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use crate::login::{normalize_password_for_storage, verify_password};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::AppState;
use gproxy_storage::Scope;
use gproxy_storage::repository::UserRepository;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct MemoryUserRow {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub is_admin: bool,
}

#[derive(Serialize)]
pub struct MemoryUserKeyRow {
    pub id: i64,
    pub user_id: i64,
    pub api_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}

#[derive(serde::Deserialize, Default)]
pub struct UserQueryParams {
    #[serde(default)]
    pub id: Scope<i64>,
    #[serde(default)]
    pub name: Scope<String>,
}

pub async fn query_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UserQueryParams>,
) -> Result<Json<Vec<MemoryUserRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let users = state.users_snapshot();
    let rows: Vec<MemoryUserRow> = users
        .iter()
        .filter(|u| match &query.id {
            Scope::Eq(v) => u.id == *v,
            _ => true,
        })
        .filter(|u| match &query.name {
            Scope::Eq(v) => u.name == *v,
            _ => true,
        })
        .map(|u| MemoryUserRow {
            id: u.id,
            name: u.name.clone(),
            enabled: u.enabled,
            is_admin: u.is_admin,
        })
        .collect();
    Ok(Json(rows))
}

pub async fn upsert_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut payload): Json<gproxy_storage::UserWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let previous = state.find_user(payload.id);
    payload.password = normalize_password_for_update(previous.as_ref(), &payload.password);
    let revoke_sessions = should_revoke_sessions(previous.as_ref(), &payload);
    state.storage().upsert_user(payload.clone()).await?;
    state.upsert_user_in_memory(gproxy_server::MemoryUser {
        id: payload.id,
        name: payload.name.clone(),
        enabled: payload.enabled,
        is_admin: payload.is_admin,
        password_hash: payload.password.clone(),
    });
    if revoke_sessions {
        state.revoke_sessions_for_user(payload.id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeleteUserPayload {
    id: i64,
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteUserPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state.storage().delete_user(payload.id).await?;
    state.remove_user_from_memory(payload.id);
    state.revoke_sessions_for_user(payload.id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize, Default)]
pub struct UserKeyQueryParams {
    #[serde(default)]
    pub user_id: Scope<i64>,
}

pub async fn query_user_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UserKeyQueryParams>,
) -> Result<Json<Vec<MemoryUserKeyRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let keys = state.keys_snapshot();
    let rows: Vec<MemoryUserKeyRow> = keys
        .values()
        .filter(|k| match &query.user_id {
            Scope::Eq(v) => k.user_id == *v,
            _ => true,
        })
        .map(|k| MemoryUserKeyRow {
            id: k.id,
            user_id: k.user_id,
            api_key: k.api_key.clone(),
            label: k.label.clone(),
            enabled: k.enabled,
        })
        .collect();
    Ok(Json(rows))
}

#[derive(serde::Deserialize)]
pub struct GenerateUserKeyPayload {
    pub user_id: i64,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(serde::Serialize)]
pub struct GenerateUserKeyResponse {
    pub ok: bool,
    pub id: i64,
    pub api_key: String,
}

pub async fn generate_user_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<GenerateUserKeyPayload>,
) -> Result<Json<GenerateUserKeyResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let api_key = generate_unique_api_key_for(&state);
    let id = state
        .storage()
        .create_user_key(payload.user_id, &api_key, payload.label.as_deref(), true)
        .await?;
    state.upsert_key_in_memory(gproxy_server::MemoryUserKey {
        id,
        user_id: payload.user_id,
        api_key: api_key.clone(),
        label: payload.label.clone(),
        enabled: true,
    });
    Ok(Json(GenerateUserKeyResponse {
        ok: true,
        id,
        api_key,
    }))
}

#[derive(serde::Deserialize)]
pub struct DeleteUserKeyPayload {
    id: i64,
}

#[derive(serde::Deserialize)]
pub struct UpdateUserKeyEnabledPayload {
    pub id: i64,
    pub enabled: bool,
}

pub async fn update_user_key_enabled(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UpdateUserKeyEnabledPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let key = state
        .keys_snapshot()
        .values()
        .find(|key| key.id == payload.id)
        .cloned()
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
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn delete_user_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteUserKeyPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state.storage().delete_user_key(payload.id).await?;
    state.remove_key_from_memory(payload.id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<gproxy_storage::UserWrite>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for mut item in items {
        let previous = state.find_user(item.id);
        item.password = normalize_password_for_update(previous.as_ref(), &item.password);
        let revoke_sessions = should_revoke_sessions(previous.as_ref(), &item);
        state.storage().upsert_user(item.clone()).await?;
        state.upsert_user_in_memory(gproxy_server::MemoryUser {
            id: item.id,
            name: item.name.clone(),
            enabled: item.enabled,
            is_admin: item.is_admin,
            password_hash: item.password.clone(),
        });
        if revoke_sessions {
            state.revoke_sessions_for_user(item.id);
        }
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for id in &ids {
        state.storage().delete_user(*id).await?;
        state.remove_user_from_memory(*id);
        state.revoke_sessions_for_user(*id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_user_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for id in &ids {
        state.storage().delete_user_key(*id).await?;
        state.remove_key_from_memory(*id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct BatchGenerateUserKeysPayload {
    pub user_id: i64,
    pub count: usize,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(serde::Serialize)]
pub struct BatchGenerateUserKeysResponse {
    pub ok: bool,
    pub keys: Vec<GeneratedKey>,
}

#[derive(serde::Serialize)]
pub struct GeneratedKey {
    pub id: i64,
    pub api_key: String,
}

pub async fn batch_upsert_user_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<BatchGenerateUserKeysPayload>,
) -> Result<Json<BatchGenerateUserKeysResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let mut keys = Vec::with_capacity(payload.count);
    for _ in 0..payload.count {
        let api_key = generate_unique_api_key_for(&state);
        let id = state
            .storage()
            .create_user_key(payload.user_id, &api_key, payload.label.as_deref(), true)
            .await?;
        state.upsert_key_in_memory(gproxy_server::MemoryUserKey {
            id,
            user_id: payload.user_id,
            api_key: api_key.clone(),
            label: payload.label.clone(),
            enabled: true,
        });
        keys.push(GeneratedKey { id, api_key });
    }
    Ok(Json(BatchGenerateUserKeysResponse { ok: true, keys }))
}

/// Generate a unique API key in `sk-api01-{random hex}` format.
pub fn generate_unique_api_key_for(state: &AppState) -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    loop {
        let n: u64 = rng.random_range(0..1u64 << 48);
        let key = format!("sk-api01-{n:012x}");
        // Use authenticate_api_key which does SHA-256 digest lookup
        if state.authenticate_api_key(&key).is_some() {
            continue;
        }
        return key;
    }
}

fn normalize_password_for_update(
    previous: Option<&gproxy_server::MemoryUser>,
    password_or_hash: &str,
) -> String {
    if let Some(previous) = previous
        && password_or_hash.trim().is_empty()
    {
        return previous.password_hash.clone();
    }
    if let Some(previous) = previous
        && (password_or_hash == previous.password_hash
            || verify_password(password_or_hash, &previous.password_hash))
    {
        return previous.password_hash.clone();
    }
    normalize_password_for_storage(password_or_hash)
}

fn should_revoke_sessions(
    previous: Option<&gproxy_server::MemoryUser>,
    payload: &gproxy_storage::UserWrite,
) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    previous.enabled != payload.enabled
        || previous.is_admin != payload.is_admin
        || previous.password_hash != payload.password
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{Json, extract::State};
    use http::HeaderMap;

    use super::{
        batch_delete_users, batch_upsert_users, delete_user, update_user_key_enabled, upsert_user,
    };
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
                id: 1,
                name: "admin".to_string(),
                password: crate::login::hash_password("admin-password"),
                enabled: true,
                is_admin: true,
            })
            .await
            .expect("seed admin");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 10,
                user_id: 1,
                api_key: "sk-admin".to_string(),
                label: Some("admin".to_string()),
                enabled: true,
            })
            .await
            .expect("seed admin key");
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

        let state = Arc::new(
            AppStateBuilder::new()
                .engine(gproxy_sdk::provider::engine::GproxyEngine::builder().build())
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

    fn admin_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-admin".parse().expect("header"));
        headers
    }

    #[tokio::test]
    async fn upsert_user_revokes_sessions_when_password_changes() {
        let state = build_test_state().await;
        let token = state.create_session(2, 60);

        let _ = upsert_user(
            State(state.clone()),
            admin_headers(),
            Json(gproxy_storage::UserWrite {
                id: 2,
                name: "alice".to_string(),
                password: "new-password".to_string(),
                enabled: true,
                is_admin: false,
            }),
        )
        .await
        .expect("upsert user");

        assert!(state.validate_session(&token).is_none());
    }

    #[tokio::test]
    async fn upsert_user_keeps_sessions_when_only_name_changes() {
        let state = build_test_state().await;
        let token = state.create_session(2, 60);
        let existing_password = state.find_user(2).expect("existing user").password_hash;

        let _ = upsert_user(
            State(state.clone()),
            admin_headers(),
            Json(gproxy_storage::UserWrite {
                id: 2,
                name: "alice-renamed".to_string(),
                password: existing_password,
                enabled: true,
                is_admin: false,
            }),
        )
        .await
        .expect("upsert user");

        assert!(state.validate_session(&token).is_some());
    }

    #[tokio::test]
    async fn upsert_user_keeps_password_when_password_is_blank() {
        let state = build_test_state().await;
        let token = state.create_session(2, 60);
        let existing_password = state.find_user(2).expect("existing user").password_hash;

        let _ = upsert_user(
            State(state.clone()),
            admin_headers(),
            Json(gproxy_storage::UserWrite {
                id: 2,
                name: "alice-renamed".to_string(),
                password: "".to_string(),
                enabled: true,
                is_admin: false,
            }),
        )
        .await
        .expect("upsert user");

        let updated = state.find_user(2).expect("updated user");
        assert_eq!(updated.password_hash, existing_password);
        assert!(state.validate_session(&token).is_some());
    }

    #[tokio::test]
    async fn delete_user_revokes_sessions() {
        let state = build_test_state().await;
        let token = state.create_session(2, 60);

        let _ = delete_user(
            State(state.clone()),
            admin_headers(),
            Json(super::DeleteUserPayload { id: 2 }),
        )
        .await
        .expect("delete user");

        assert!(state.validate_session(&token).is_none());
    }

    #[tokio::test]
    async fn batch_upsert_users_revokes_sessions_when_role_changes() {
        let state = build_test_state().await;
        let token = state.create_session(2, 60);
        let existing_password = state.find_user(2).expect("existing user").password_hash;

        let _ = batch_upsert_users(
            State(state.clone()),
            admin_headers(),
            Json(vec![gproxy_storage::UserWrite {
                id: 2,
                name: "alice".to_string(),
                password: existing_password,
                enabled: true,
                is_admin: true,
            }]),
        )
        .await
        .expect("batch upsert users");

        assert!(state.validate_session(&token).is_none());
    }

    #[tokio::test]
    async fn batch_delete_users_revokes_sessions() {
        let state = build_test_state().await;
        let token = state.create_session(2, 60);

        let _ = batch_delete_users(State(state.clone()), admin_headers(), Json(vec![2]))
            .await
            .expect("batch delete users");

        assert!(state.validate_session(&token).is_none());
    }

    #[tokio::test]
    async fn update_user_key_enabled_updates_memory() {
        let state = build_test_state().await;
        state.upsert_key_in_memory(gproxy_server::MemoryUserKey {
            id: 20,
            user_id: 2,
            api_key: "sk-user".to_string(),
            label: Some("user".to_string()),
            enabled: true,
        });

        let _ = update_user_key_enabled(
            State(state.clone()),
            admin_headers(),
            Json(super::UpdateUserKeyEnabledPayload {
                id: 20,
                enabled: false,
            }),
        )
        .await
        .expect("update user key enabled");

        let key = state
            .keys_for_user(2)
            .into_iter()
            .find(|key| key.id == 20)
            .expect("updated key");
        assert!(!key.enabled);
    }
}
