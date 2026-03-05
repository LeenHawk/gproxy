use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde::Serialize;

use crate::AppState;

use super::error::HttpError;

const X_API_KEY: &str = "x-api-key";

#[derive(Debug, Serialize)]
struct Ack {
    ok: bool,
}

#[derive(Debug, Deserialize)]
struct DeleteMyKeyPayload {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct ChangeMyPasswordPayload {
    current_password: String,
    new_password: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/keys/query", post(query_my_keys))
        .route("/keys/generate", post(generate_my_key))
        .route("/keys/delete", post(delete_my_key))
        .route("/password/change", post(change_my_password))
        .route("/usages/query", post(query_my_usages))
        .route("/usages/count", post(count_my_usages))
        .route("/usages/summary", post(summarize_my_usages))
}

fn api_key_from_headers(headers: &HeaderMap) -> Result<&str, HttpError> {
    gproxy_admin::extract_api_key(headers.get(X_API_KEY).and_then(|value| value.to_str().ok()))
        .map_err(Into::into)
}

async fn query_my_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<gproxy_storage::UserKeyQueryRow>>, HttpError> {
    let api_key = api_key_from_headers(&headers)?;
    let users = state.load_users();
    let keys = state.load_keys();
    Ok(Json(
        gproxy_admin::query_my_user_keys(&users, &keys, api_key).await?,
    ))
}

async fn generate_my_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<gproxy_storage::UserKeyWrite>, HttpError> {
    let api_key = api_key_from_headers(&headers)?;
    let users = state.load_users();
    let keys = state.load_keys();
    let me = gproxy_admin::authenticate_user_key(&users, &keys, api_key).await?;
    let mut id = None;
    let mut generated = None;
    for _ in 0..8 {
        let candidate = gproxy_admin::generate_unique_user_api_key(&keys)?;
        let create_result = state
            .load_storage()
            .create_user_key(me.user_id, candidate.as_str(), Some("auto-generated"), true)
            .await;
        match create_result {
            Ok(created_id) => {
                id = Some(created_id);
                generated = Some(candidate);
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
    let generated = generated.ok_or_else(|| {
        HttpError::from(gproxy_admin::AdminApiError::InvalidInput(
            "failed to generate unique user key".to_string(),
        ))
    })?;
    let write = gproxy_storage::UserKeyWrite {
        id,
        user_id: me.user_id,
        api_key: generated,
        label: Some("auto-generated".to_string()),
        enabled: true,
    };
    state.upsert_user_key_in_memory(write.clone());
    Ok(Json(write))
}

async fn delete_my_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteMyKeyPayload>,
) -> Result<Json<Ack>, HttpError> {
    let api_key = api_key_from_headers(&headers)?;
    let users = state.load_users();
    let keys = state.load_keys();
    gproxy_admin::delete_my_user_key(&state.storage_writes, api_key, &users, &keys, payload.id)
        .await?;
    state.delete_user_key_in_memory(payload.id);
    Ok(Json(Ack { ok: true }))
}

async fn change_my_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<ChangeMyPasswordPayload>,
) -> Result<Json<Ack>, HttpError> {
    let api_key = api_key_from_headers(&headers)?;
    let users = state.load_users();
    let keys = state.load_keys();
    let me = gproxy_admin::authenticate_user_key(&users, &keys, api_key).await?;
    let Some(user) = users.iter().find(|row| row.id == me.user_id) else {
        return Err(HttpError::from(gproxy_admin::AdminApiError::Unauthorized));
    };

    if payload.current_password.trim() != user.password {
        return Err(HttpError::from(gproxy_admin::AdminApiError::Unauthorized));
    }

    let new_password = payload.new_password.trim();
    if new_password.is_empty() {
        return Err(HttpError::from(gproxy_admin::AdminApiError::InvalidInput(
            "new password cannot be empty".to_string(),
        )));
    }

    let write = gproxy_storage::UserWrite {
        id: user.id,
        name: user.name.clone(),
        password: new_password.to_string(),
        enabled: user.enabled,
    };
    state.upsert_user_in_memory(write.clone());
    gproxy_admin::upsert_user(&state.storage_writes, write).await?;
    Ok(Json(Ack { ok: true }))
}

async fn query_my_usages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<gproxy_storage::UsageQuery>,
) -> Result<Json<Vec<gproxy_storage::UsageQueryRow>>, HttpError> {
    let api_key = api_key_from_headers(&headers)?;
    let users = state.load_users();
    let keys = state.load_keys();
    let storage = state.load_storage();
    Ok(Json(
        gproxy_admin::query_my_usages(&storage, &users, &keys, api_key, query).await?,
    ))
}

async fn summarize_my_usages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<gproxy_storage::UsageQuery>,
) -> Result<Json<gproxy_storage::UsageSummary>, HttpError> {
    let api_key = api_key_from_headers(&headers)?;
    let users = state.load_users();
    let keys = state.load_keys();
    let storage = state.load_storage();
    Ok(Json(
        gproxy_admin::summarize_my_usages(&storage, &users, &keys, api_key, query).await?,
    ))
}

async fn count_my_usages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<gproxy_storage::UsageQuery>,
) -> Result<Json<gproxy_storage::UsageQueryCount>, HttpError> {
    let api_key = api_key_from_headers(&headers)?;
    let users = state.load_users();
    let keys = state.load_keys();
    let storage = state.load_storage();
    Ok(Json(
        gproxy_admin::count_my_usages(&storage, &users, &keys, api_key, query).await?,
    ))
}
