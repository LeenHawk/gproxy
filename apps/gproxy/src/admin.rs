use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use time::OffsetDateTime;
use tokio::sync::watch;

use gproxy_core::{AuthKeyEntry, AuthSnapshot, MemoryAuth, UserEntry};
use gproxy_provider_core::{
    CredentialEntry, DisallowEntry, DisallowKey, DisallowLevel, DisallowScope, PoolSnapshot,
};
use gproxy_provider_impl::{BaseCredential, ProviderRegistry};
use gproxy_storage::{
    entities, AdminCredentialInput, AdminDisallowInput, AdminKeyInput, AdminProviderInput,
    AdminUserInput, TrafficStorage,
};

use crate::cli::GlobalConfig;
use crate::dsn::ensure_sqlite_dsn;
use crate::snapshot;

#[derive(Clone)]
struct AdminState {
    storage: Arc<RwLock<TrafficStorage>>,
    config: Arc<RwLock<GlobalConfig>>,
    bind_tx: watch::Sender<String>,
    registry: Arc<ProviderRegistry>,
    auth: Arc<MemoryAuth>,
    provider_ids: Arc<RwLock<HashMap<String, i64>>>,
    provider_names: Arc<RwLock<HashMap<i64, String>>>,
}

pub(crate) fn admin_router(
    config: Arc<RwLock<GlobalConfig>>,
    storage: TrafficStorage,
    bind_tx: watch::Sender<String>,
    registry: Arc<ProviderRegistry>,
    auth: Arc<MemoryAuth>,
    provider_ids: HashMap<String, i64>,
    provider_names: HashMap<i64, String>,
) -> Router {
    let state = AdminState {
        storage: Arc::new(RwLock::new(storage)),
        config,
        bind_tx,
        registry,
        auth,
        provider_ids: Arc::new(RwLock::new(provider_ids)),
        provider_names: Arc::new(RwLock::new(provider_names)),
    };

    Router::new()
        .route("/admin/health", get(admin_health))
        .route("/admin/config", get(get_config).put(put_config))
        .route(
            "/admin/providers",
            get(list_providers).post(create_provider),
        )
        .route(
            "/admin/providers/{id}",
            put(update_provider).delete(delete_provider),
        )
        .route(
            "/admin/credentials",
            get(list_credentials).post(create_credential),
        )
        .route(
            "/admin/credentials/{id}",
            put(update_credential).delete(delete_credential),
        )
        .route("/admin/disallow", get(list_disallow).post(create_disallow))
        .route("/admin/disallow/{id}", delete(delete_disallow))
        .route("/admin/users", get(list_users).post(create_user))
        .route("/admin/users/{id}", delete(delete_user))
        .route("/admin/keys", get(list_keys).post(create_key))
        .route("/admin/keys/{id}", delete(delete_key))
        .route("/admin/keys/{id}/disable", put(disable_key))
        .route("/admin/reload", post(reload_snapshot))
        .route("/admin/stats", get(stats))
        .with_state(state)
}

#[allow(clippy::result_large_err)]
impl AdminState {
    fn storage(&self) -> Result<TrafficStorage, Response> {
        self.storage
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage lock poisoned",
                )
                    .into_response()
            })
    }

    fn set_storage(&self, storage: TrafficStorage) -> Result<(), Response> {
        self.storage
            .write()
            .map(|mut guard| {
                *guard = storage;
            })
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage lock poisoned",
                )
                    .into_response()
            })
    }

    fn dsn(&self) -> Result<String, Response> {
        self.config().map(|config| config.dsn)
    }

    fn config(&self) -> Result<GlobalConfig, Response> {
        self.config
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "config lock poisoned",
                )
                    .into_response()
            })
    }

    fn set_config(&self, config: GlobalConfig) -> Result<(), Response> {
        self.config
            .write()
            .map(|mut guard| {
                *guard = config;
            })
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "config lock poisoned",
                )
                    .into_response()
            })
    }

    fn admin_key(&self) -> Result<String, Response> {
        self.config().map(|config| config.admin_key)
    }
}

async fn admin_health(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.health().await {
        Ok(_) => Json(json!({ "status": "ok" })).into_response(),
        Err(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("db error: {err}"),
        )
            .into_response(),
    }
}

async fn get_config(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.get_global_config().await {
        Ok(Some(cfg)) => Json(json!({
            "id": cfg.id,
            "config_json": cfg.config_json,
            "updated_at": ts(cfg.updated_at),
        }))
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "global_config not found").into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn put_config(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(payload): Json<GlobalConfig>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let current_dsn = match state.dsn() {
        Ok(dsn) => dsn,
        Err(resp) => return resp,
    };

    let current_config = match state.config() {
        Ok(config) => config,
        Err(resp) => return resp,
    };

    let mut desired = payload;
    if desired.dsn.trim().is_empty() {
        desired.dsn = current_dsn.clone();
    }
    if desired.data_dir.trim().is_empty() {
        desired.data_dir = current_config.data_dir.clone();
    }
    if desired.dsn.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "dsn cannot be empty",
        )
            .into_response();
    }

    let config_json = match serde_json::to_value(&desired) {
        Ok(value) => value,
        Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    };

    let dsn_changed = desired.dsn != current_dsn;
    let data_dir_changed = desired.data_dir != current_config.data_dir;
    let bind_changed =
        desired.host != current_config.host || desired.port != current_config.port;
    let proxy_changed = desired.proxy != current_config.proxy;

    let effective_storage = if dsn_changed {
        if let Err(err) = ensure_sqlite_dsn(&desired.dsn) {
            return (StatusCode::BAD_REQUEST, err.to_string()).into_response();
        }
        let new_storage = match TrafficStorage::connect(&desired.dsn).await {
            Ok(storage) => storage,
            Err(err) => {
                return (StatusCode::BAD_REQUEST, err.to_string()).into_response()
            }
        };
        if let Err(err) = new_storage.sync().await {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
        new_storage
    } else {
        storage.clone()
    };

    if let Err(err) = effective_storage
        .upsert_global_config(1, config_json, OffsetDateTime::now_utc())
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }

    if let Err(err) = effective_storage
        .ensure_admin_user(&desired.admin_key)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }

    let snapshot = match effective_storage.load_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    apply_snapshot(&state, &snapshot);

    if dsn_changed && let Err(resp) = state.set_storage(effective_storage) {
        return resp;
    }
    if let Err(resp) = state.set_config(desired.clone()) {
        return resp;
    }
    if bind_changed {
        let bind = format!("{}:{}", desired.host, desired.port);
        if state.bind_tx.send(bind).is_err() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "bind channel closed",
            )
                .into_response();
        }
    }
    let _ = data_dir_changed;

    Json(json!({
        "status": "ok",
        "dsn_changed": dsn_changed,
        "bind_changed": bind_changed,
        "proxy_changed": proxy_changed,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct ProviderPayload {
    id: Option<i64>,
    name: String,
    config_json: JsonValue,
    enabled: bool,
}

async fn list_providers(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.list_providers().await {
        Ok(items) => {
            let data: Vec<JsonValue> = items.into_iter().map(provider_to_json).collect();
            Json(json!(data)).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn create_provider(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(payload): Json<ProviderPayload>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let name = payload.name;
    let input = AdminProviderInput {
        id: payload.id,
        name: name.clone(),
        config_json: payload.config_json,
        enabled: payload.enabled,
    };

    match storage.upsert_provider(input).await {
        Ok(id) => {
            insert_provider_map(&state, id, name.clone());
            let _ = refresh_provider_pool(&state, &storage, Some(id)).await;
            Json(json!({ "status": "ok" })).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn update_provider(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<ProviderPayload>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let name = payload.name;
    let input = AdminProviderInput {
        id: Some(id),
        name: name.clone(),
        config_json: payload.config_json,
        enabled: payload.enabled,
    };

    match storage.upsert_provider(input).await {
        Ok(id) => {
            update_provider_map(&state, id, name);
            if let Err(resp) = refresh_provider_pool(&state, &storage, Some(id)).await {
                return resp;
            }
            Json(json!({ "status": "ok" })).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_provider(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let providers = match storage.list_providers().await {
        Ok(items) => items,
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    };
    let name = match providers.iter().find(|provider| provider.id == id) {
        Some(provider) => provider.name.clone(),
        None => return (StatusCode::NOT_FOUND, "provider not found").into_response(),
    };

    match storage.delete_provider(id).await {
        Ok(_) => {
            clear_provider_pool(&state, &name);
            remove_provider_map(&state, id);
            Json(json!({ "status": "ok" })).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CredentialPayload {
    id: Option<i64>,
    provider_id: Option<i64>,
    provider_name: Option<String>,
    name: Option<String>,
    secret: JsonValue,
    meta_json: JsonValue,
    weight: i32,
    enabled: bool,
}

async fn list_credentials(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.list_credentials().await {
        Ok(items) => {
            let data: Vec<JsonValue> = items.into_iter().map(credential_to_json).collect();
            Json(json!(data)).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn create_credential(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(payload): Json<CredentialPayload>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let provider_id = match resolve_provider_id(&state, payload.provider_id, payload.provider_name) {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let input = AdminCredentialInput {
        id: payload.id,
        provider_id,
        name: payload.name,
        secret: payload.secret,
        meta_json: payload.meta_json,
        weight: payload.weight,
        enabled: payload.enabled,
    };

    match storage.upsert_credential(input).await {
        Ok(_) => match refresh_provider_pool(&state, &storage, Some(provider_id)).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn update_credential(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<CredentialPayload>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let provider_id = match resolve_provider_id(&state, payload.provider_id, payload.provider_name) {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let input = AdminCredentialInput {
        id: Some(id),
        provider_id,
        name: payload.name,
        secret: payload.secret,
        meta_json: payload.meta_json,
        weight: payload.weight,
        enabled: payload.enabled,
    };

    match storage.upsert_credential(input).await {
        Ok(_) => match refresh_provider_pool(&state, &storage, Some(provider_id)).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_credential(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let credentials = match storage.list_credentials().await {
        Ok(items) => items,
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    };
    let provider_id = match credentials.iter().find(|cred| cred.id == id) {
        Some(cred) => cred.provider_id,
        None => return (StatusCode::NOT_FOUND, "credential not found").into_response(),
    };

    match storage.delete_credential(id).await {
        Ok(_) => match refresh_provider_pool(&state, &storage, Some(provider_id)).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct DisallowPayload {
    credential_id: i64,
    scope_kind: String,
    scope_value: Option<String>,
    level: String,
    until_at: Option<i64>,
    reason: Option<String>,
}

async fn list_disallow(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.list_disallow().await {
        Ok(items) => {
            let data: Vec<JsonValue> = items.into_iter().map(disallow_to_json).collect();
            Json(json!(data)).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn create_disallow(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(payload): Json<DisallowPayload>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let until_at = payload
        .until_at
        .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok());
    let input = AdminDisallowInput {
        credential_id: payload.credential_id,
        scope_kind: payload.scope_kind,
        scope_value: payload.scope_value,
        level: payload.level,
        until_at,
        reason: payload.reason,
    };

    let provider_id = match provider_id_for_credential(&storage, payload.credential_id).await {
        Ok(provider_id) => provider_id,
        Err(resp) => return resp,
    };

    match storage.upsert_disallow(input).await {
        Ok(_) => match refresh_provider_pool(&state, &storage, Some(provider_id)).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_disallow(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let disallow = match storage.list_disallow().await {
        Ok(items) => items,
        Err(err) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    };
    let credential_id = match disallow.iter().find(|item| item.id == id) {
        Some(item) => item.credential_id,
        None => return (StatusCode::NOT_FOUND, "disallow not found").into_response(),
    };
    let provider_id = match provider_id_for_credential(&storage, credential_id).await {
        Ok(provider_id) => provider_id,
        Err(resp) => return resp,
    };

    match storage.delete_disallow(id).await {
        Ok(_) => match refresh_provider_pool(&state, &storage, Some(provider_id)).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct UserPayload {
    id: Option<i64>,
    name: Option<String>,
}

async fn list_users(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.list_users().await {
        Ok(items) => {
            let data: Vec<JsonValue> = items.into_iter().map(user_to_json).collect();
            Json(json!(data)).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn create_user(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(payload): Json<UserPayload>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let input = AdminUserInput {
        id: payload.id,
        name: payload.name,
    };

    match storage.upsert_user(input).await {
        Ok(_) => match refresh_auth(&state, &storage).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_user(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.delete_user(id).await {
        Ok(_) => match refresh_auth(&state, &storage).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct KeyPayload {
    id: Option<i64>,
    user_id: i64,
    key_value: String,
    label: Option<String>,
    enabled: Option<bool>,
}

async fn list_keys(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.list_keys().await {
        Ok(items) => {
            let data: Vec<JsonValue> = items.into_iter().map(key_to_json).collect();
            Json(json!(data)).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn create_key(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Json(payload): Json<KeyPayload>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let input = AdminKeyInput {
        id: payload.id,
        user_id: payload.user_id,
        key_value: payload.key_value,
        label: payload.label,
        enabled: payload.enabled.unwrap_or(true),
    };

    match storage.upsert_key(input).await {
        Ok(_) => match refresh_auth(&state, &storage).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn delete_key(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.delete_key(id).await {
        Ok(_) => match refresh_auth(&state, &storage).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn disable_key(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    match storage.set_key_enabled(id, false).await {
        Ok(_) => match refresh_auth(&state, &storage).await {
            Ok(_) => Json(json!({ "status": "ok" })).into_response(),
            Err(resp) => resp,
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn reload_snapshot(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let storage = match state.storage() {
        Ok(storage) => storage,
        Err(resp) => return resp,
    };

    let snapshot = match storage.load_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    apply_snapshot(&state, &snapshot);

    Json(json!({ "status": "ok" })).into_response()
}

#[derive(Serialize)]
struct ProviderPoolStats {
    name: String,
    credentials_total: usize,
    credentials_enabled: usize,
    disallow: usize,
}

async fn stats(
    State(state): State<AdminState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_admin(&state, &headers) {
        return resp;
    }

    let stats = collect_stats(&state);
    Json(json!({ "providers": stats })).into_response()
}

fn collect_stats(state: &AdminState) -> Vec<ProviderPoolStats> {
    let mut out = Vec::new();
    collect_one(&mut out, "openai", state.registry.openai().pool().snapshot());
    collect_one(&mut out, "claude", state.registry.claude().pool().snapshot());
    collect_one(
        &mut out,
        "aistudio",
        state.registry.aistudio().pool().snapshot(),
    );
    collect_one(
        &mut out,
        "vertexexpress",
        state.registry.vertexexpress().pool().snapshot(),
    );
    collect_one(&mut out, "vertex", state.registry.vertex().pool().snapshot());
    collect_one(
        &mut out,
        "geminicli",
        state.registry.geminicli().pool().snapshot(),
    );
    collect_one(
        &mut out,
        "claudecode",
        state.registry.claudecode().pool().snapshot(),
    );
    collect_one(&mut out, "codex", state.registry.codex().pool().snapshot());
    collect_one(
        &mut out,
        "antigravity",
        state.registry.antigravity().pool().snapshot(),
    );
    collect_one(
        &mut out,
        "nvidia",
        state.registry.nvidia().pool().snapshot(),
    );
    collect_one(
        &mut out,
        "deepseek",
        state.registry.deepseek().pool().snapshot(),
    );
    out
}

fn apply_snapshot(state: &AdminState, snapshot: &gproxy_storage::StorageSnapshot) {
    let auth_snapshot = snapshot::build_auth_snapshot(snapshot);
    state.auth.replace_snapshot(auth_snapshot);
    let pools = snapshot::build_provider_pools(snapshot);
    state.registry.apply_pools(pools);
    let provider_ids = snapshot::build_provider_id_map(snapshot);
    let provider_names = snapshot::build_provider_name_map(snapshot);
    if let Ok(mut guard) = state.provider_ids.write() {
        *guard = provider_ids;
    }
    if let Ok(mut guard) = state.provider_names.write() {
        *guard = provider_names;
    }
}

#[allow(clippy::result_large_err)]
async fn refresh_auth(
    state: &AdminState,
    storage: &TrafficStorage,
) -> Result<(), Response> {
    let users = match storage.list_users().await {
        Ok(items) => items,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()),
    };
    let keys = match storage.list_keys().await {
        Ok(items) => items,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()),
    };

    let mut snapshot = AuthSnapshot::default();
    for key in keys {
        snapshot.keys_by_value.insert(
            key.key_value,
            AuthKeyEntry {
                key_id: key.id,
                user_id: key.user_id,
                enabled: key.enabled,
            },
        );
    }
    for user in users {
        snapshot.users_by_id.insert(
            user.id,
            UserEntry {
                id: user.id,
                name: user.name,
            },
        );
    }
    state.auth.replace_snapshot(snapshot);
    Ok(())
}

#[allow(clippy::result_large_err)]
async fn provider_id_for_credential(
    storage: &TrafficStorage,
    credential_id: i64,
) -> Result<i64, Response> {
    let credentials = match storage.list_credentials().await {
        Ok(items) => items,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()),
    };
    match credentials.iter().find(|cred| cred.id == credential_id) {
        Some(cred) => Ok(cred.provider_id),
        None => Err((StatusCode::NOT_FOUND, "credential not found").into_response()),
    }
}

#[allow(clippy::result_large_err)]
fn resolve_provider_id(
    state: &AdminState,
    provider_id: Option<i64>,
    provider_name: Option<String>,
) -> Result<i64, Response> {
    if let Some(id) = provider_id {
        return Ok(id);
    }
    let Some(name) = provider_name else {
        return Err((StatusCode::BAD_REQUEST, "provider_id or provider_name required").into_response());
    };
    let map = state
        .provider_ids
        .read()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "provider map poisoned").into_response())?;
    match map.get(&name) {
        Some(id) => Ok(*id),
        None => Err((StatusCode::NOT_FOUND, "provider not found").into_response()),
    }
}

fn insert_provider_map(state: &AdminState, id: i64, name: String) {
    if let Ok(mut guard) = state.provider_ids.write() {
        guard.insert(name.clone(), id);
    }
    if let Ok(mut guard) = state.provider_names.write() {
        guard.insert(id, name);
    }
}

fn update_provider_map(state: &AdminState, id: i64, name: String) {
    if let Ok(mut guard) = state.provider_names.write()
        && let Some(old_name) = guard.insert(id, name.clone()) {
            if let Ok(mut ids) = state.provider_ids.write() {
                ids.remove(&old_name);
                ids.insert(name.clone(), id);
            }
            return;
        }
    if let Ok(mut guard) = state.provider_ids.write() {
        guard.insert(name.clone(), id);
    }
    if let Ok(mut guard) = state.provider_names.write() {
        guard.insert(id, name);
    }
}

fn remove_provider_map(state: &AdminState, id: i64) {
    if let Ok(mut guard) = state.provider_names.write()
        && let Some(name) = guard.remove(&id)
            && let Ok(mut ids) = state.provider_ids.write() {
                ids.remove(&name);
            }
}

fn clear_provider_pool(state: &AdminState, name: &str) {
    let empty = PoolSnapshot::new(Vec::new(), HashMap::new());
    let mut pools = HashMap::new();
    pools.insert(name.to_string(), empty);
    state.registry.apply_pools(pools);
}

fn parse_disallow_scope(kind: &str, value: Option<&str>) -> Option<DisallowScope> {
    match kind {
        "all_models" | "all" => Some(DisallowScope::AllModels),
        "model" => value.map(|model| DisallowScope::Model(model.to_string())),
        _ => None,
    }
}

fn parse_disallow_level(level: &str) -> Option<DisallowLevel> {
    match level {
        "cooldown" => Some(DisallowLevel::Cooldown),
        "transient" => Some(DisallowLevel::Transient),
        "dead" => Some(DisallowLevel::Dead),
        _ => None,
    }
}

fn to_system_time(value: OffsetDateTime) -> Option<SystemTime> {
    let ts = value.unix_timestamp();
    if ts < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(ts as u64))
}

#[allow(clippy::result_large_err)]
async fn refresh_provider_pool(
    state: &AdminState,
    storage: &TrafficStorage,
    provider_id: Option<i64>,
) -> Result<(), Response> {
    let provider_id = match provider_id {
        Some(id) => id,
        None => return Ok(()),
    };

    let provider_name = {
        let map = state
            .provider_names
            .read()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "provider map poisoned").into_response())?;
        match map.get(&provider_id) {
            Some(name) => name.clone(),
            None => return Err((StatusCode::NOT_FOUND, "provider not found").into_response()),
        }
    };

    let credentials = match storage.list_credentials().await {
        Ok(items) => items,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()),
    };
    let disallow = match storage.list_disallow().await {
        Ok(items) => items,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()),
    };

    let mut entries = Vec::new();
    let mut credential_ids = HashSet::new();

    for credential in credentials.iter().filter(|cred| cred.provider_id == provider_id) {
        credential_ids.insert(credential.id);
        let weight = if credential.weight >= 0 {
            credential.weight as u32
        } else {
            0
        };
        let entry = CredentialEntry::new(
            credential.id.to_string(),
            credential.enabled,
            weight,
            BaseCredential {
                id: credential.id,
                name: credential.name.clone(),
                secret: credential.secret.clone(),
                meta: credential.meta_json.clone(),
            },
        );
        entries.push(entry);
    }

    let mut disallow_map = HashMap::new();
    for record in disallow
        .iter()
        .filter(|item| credential_ids.contains(&item.credential_id))
    {
        let Some(scope) = parse_disallow_scope(
            record.scope_kind.as_str(),
            record.scope_value.as_deref(),
        ) else {
            continue;
        };
        let Some(level) = parse_disallow_level(record.level.as_str()) else {
            continue;
        };
        let entry = DisallowEntry {
            level,
            until: record.until_at.and_then(to_system_time),
            reason: record.reason.clone(),
            updated_at: to_system_time(record.updated_at).unwrap_or(SystemTime::UNIX_EPOCH),
        };
        let key = DisallowKey::new(record.credential_id.to_string(), scope);
        disallow_map.insert(key, entry);
    }

    let snapshot = PoolSnapshot::new(entries, disallow_map);
    let mut pools = HashMap::new();
    pools.insert(provider_name, snapshot);
    state.registry.apply_pools(pools);
    Ok(())
}

fn collect_one<C>(
    out: &mut Vec<ProviderPoolStats>,
    name: &str,
    snapshot: Arc<gproxy_provider_core::PoolSnapshot<C>>,
) {
    let total = snapshot.credentials.len();
    let enabled = snapshot.credentials.iter().filter(|cred| cred.enabled).count();
    let disallow = snapshot.disallow.len();
    out.push(ProviderPoolStats {
        name: name.to_string(),
        credentials_total: total,
        credentials_enabled: enabled,
        disallow,
    });
}

#[allow(clippy::result_large_err)]
fn require_admin(state: &AdminState, headers: &HeaderMap) -> Result<(), Response> {
    let admin_key = state.admin_key()?;
    if is_admin(headers, &admin_key) {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "unauthorized").into_response())
    }
}

fn is_admin(headers: &HeaderMap, admin_key: &str) -> bool {
    if let Some(value) = header_value(headers, "x-admin-key") {
        return value == admin_key;
    }

    let Some(auth) = header_value(headers, "authorization") else {
        return false;
    };
    let auth = auth.trim();
    if let Some(token) = auth.strip_prefix("Bearer ") {
        return token.trim() == admin_key;
    }
    if let Some(token) = auth.strip_prefix("bearer ") {
        return token.trim() == admin_key;
    }
    false
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn ts(value: OffsetDateTime) -> i64 {
    value.unix_timestamp()
}

fn ts_opt(value: Option<OffsetDateTime>) -> Option<i64> {
    value.map(|v| v.unix_timestamp())
}

fn provider_to_json(provider: entities::providers::Model) -> JsonValue {
    json!({
        "id": provider.id,
        "name": provider.name,
        "config_json": provider.config_json,
        "enabled": provider.enabled,
        "updated_at": ts(provider.updated_at),
    })
}

fn credential_to_json(credential: entities::credentials::Model) -> JsonValue {
    json!({
        "id": credential.id,
        "provider_id": credential.provider_id,
        "name": credential.name,
        "secret": credential.secret,
        "meta_json": credential.meta_json,
        "weight": credential.weight,
        "enabled": credential.enabled,
        "created_at": ts(credential.created_at),
        "updated_at": ts(credential.updated_at),
    })
}

fn disallow_to_json(record: entities::credential_disallow::Model) -> JsonValue {
    json!({
        "id": record.id,
        "credential_id": record.credential_id,
        "scope_kind": record.scope_kind,
        "scope_value": record.scope_value,
        "level": record.level,
        "until_at": ts_opt(record.until_at),
        "reason": record.reason,
        "updated_at": ts(record.updated_at),
    })
}

fn user_to_json(user: entities::users::Model) -> JsonValue {
    json!({
        "id": user.id,
        "name": user.name,
        "created_at": ts(user.created_at),
        "updated_at": ts(user.updated_at),
    })
}

fn key_to_json(key: entities::api_keys::Model) -> JsonValue {
    json!({
        "id": key.id,
        "user_id": key.user_id,
        "key_value": key.key_value,
        "label": key.label,
        "enabled": key.enabled,
        "created_at": ts(key.created_at),
        "last_used_at": ts_opt(key.last_used_at),
    })
}
