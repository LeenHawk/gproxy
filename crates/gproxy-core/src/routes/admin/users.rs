use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Serialize;

use crate::AppState;

use super::{Ack, DeleteById, HttpError, UpsertUserPayload, authorize_admin};

pub(super) async fn query_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<gproxy_storage::UserQuery>,
) -> Result<Json<Vec<gproxy_storage::UserQueryRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let users = state.load_users();
    Ok(Json(gproxy_admin::query_users(&users, query).await?))
}

pub(super) async fn upsert_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UpsertUserPayload>,
) -> Result<Json<UpsertUserAck>, HttpError> {
    authorize_admin(&headers, &state)?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(HttpError::from(gproxy_admin::AdminApiError::InvalidInput(
            "user name cannot be empty".to_string(),
        )));
    }
    let password = payload.password.trim();
    if password.is_empty() {
        return Err(HttpError::from(gproxy_admin::AdminApiError::InvalidInput(
            "user password cannot be empty".to_string(),
        )));
    }
    let id = if let Some(id) = payload.id {
        if !state.load_users().iter().any(|row| row.id == id) {
            return Err(HttpError::from(gproxy_admin::AdminApiError::NotFound(
                format!("user {id}"),
            )));
        }
        gproxy_admin::upsert_user(
            &state.storage_writes,
            gproxy_storage::UserWrite {
                id,
                name: name.to_string(),
                password: password.to_string(),
                enabled: payload.enabled,
            },
        )
        .await?;
        id
    } else {
        state
            .load_storage()
            .create_user(name, password, payload.enabled)
            .await
            .map_err(gproxy_admin::AdminApiError::from)?
    };
    let write = gproxy_storage::UserWrite {
        id,
        name: name.to_string(),
        password: password.to_string(),
        enabled: payload.enabled,
    };
    state.upsert_user_in_memory(write.clone());
    Ok(Json(UpsertUserAck { ok: true, id }))
}

pub(super) async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteById>,
) -> Result<Json<Ack>, HttpError> {
    authorize_admin(&headers, &state)?;
    state.delete_user_in_memory(payload.id);
    gproxy_admin::delete_user(&state.storage_writes, payload.id).await?;
    Ok(Json(Ack { ok: true }))
}

#[derive(Debug, Serialize)]
pub(super) struct UpsertUserAck {
    pub ok: bool,
    pub id: i64,
}
