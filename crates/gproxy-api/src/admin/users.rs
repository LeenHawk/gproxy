use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use crate::login::normalize_password_for_storage;
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
    payload.password = normalize_password_for_storage(&payload.password);
    state.storage().upsert_user(payload.clone()).await?;
    state.upsert_user_in_memory(gproxy_server::MemoryUser {
        id: payload.id,
        name: payload.name.clone(),
        enabled: payload.enabled,
        password_hash: payload.password.clone(),
    });
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
        item.password = normalize_password_for_storage(&item.password);
        state.storage().upsert_user(item.clone()).await?;
        state.upsert_user_in_memory(gproxy_server::MemoryUser {
            id: item.id,
            name: item.name.clone(),
            enabled: item.enabled,
            password_hash: item.password.clone(),
        });
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
/// Ensures the key doesn't collide with admin_key or existing keys.
pub fn generate_unique_api_key_for(state: &AppState) -> String {
    use rand::RngExt;
    let admin_key = state.config().admin_key.clone();
    let keys = state.keys_snapshot();
    let mut rng = rand::rng();
    loop {
        let n: u64 = rng.random_range(0..1u64 << 48);
        let key = format!("sk-api01-{n:012x}");
        if key == admin_key {
            continue;
        }
        if keys.contains_key(&key) {
            continue;
        }
        return key;
    }
}
