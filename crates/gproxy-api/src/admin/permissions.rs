use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::{AppState, PermissionEntry};
use gproxy_storage::Scope;
use gproxy_storage::repository::PermissionRepository;
use std::sync::Arc;

/// Response row for permissions from memory.
#[derive(serde::Serialize)]
pub struct MemoryPermissionRow {
    pub id: i64,
    pub user_id: i64,
    pub provider_id: Option<i64>,
    pub model_pattern: String,
}

/// Query filter for permissions.
#[derive(serde::Deserialize, Default)]
pub struct PermissionQueryParams {
    pub user_id: Option<Scope<i64>>,
    pub provider_id: Option<Scope<i64>>,
    pub limit: Option<usize>,
}

fn existing_permission_by_id(
    state: &AppState,
    permission_id: i64,
) -> Option<(i64, PermissionEntry)> {
    state
        .user_permissions_snapshot()
        .iter()
        .find_map(|(user_id, entries)| {
            entries
                .iter()
                .find(|entry| entry.id == permission_id)
                .cloned()
                .map(|entry| (*user_id, entry))
        })
}

fn existing_permission_by_key(
    state: &AppState,
    user_id: i64,
    provider_id: Option<i64>,
    model_pattern: &str,
) -> Option<PermissionEntry> {
    state
        .user_permissions_snapshot()
        .get(&user_id)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| {
                    entry.provider_id == provider_id && entry.model_pattern == model_pattern
                })
                .cloned()
        })
}

fn canonicalize_permission_write(
    state: &AppState,
    payload: &gproxy_storage::UserModelPermissionWrite,
) -> (gproxy_storage::UserModelPermissionWrite, Option<i64>) {
    let existing_by_id = existing_permission_by_id(state, payload.id);
    let existing_by_key = existing_permission_by_key(
        state,
        payload.user_id,
        payload.provider_id,
        &payload.model_pattern,
    );
    let effective_id = existing_by_key
        .as_ref()
        .map(|entry| entry.id)
        .unwrap_or(payload.id);
    let delete_id = existing_by_id
        .filter(|(_, entry)| entry.id != effective_id)
        .map(|(_, entry)| entry.id);

    (
        gproxy_storage::UserModelPermissionWrite {
            id: effective_id,
            user_id: payload.user_id,
            provider_id: payload.provider_id,
            model_pattern: payload.model_pattern.clone(),
        },
        delete_id,
    )
}

pub async fn query_permissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<PermissionQueryParams>,
) -> Result<Json<Vec<MemoryPermissionRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let perms = state.user_permissions_snapshot();
    let mut rows: Vec<MemoryPermissionRow> = Vec::new();
    for (&user_id, entries) in perms.iter() {
        // Filter by user_id
        match &query.user_id {
            Some(Scope::Eq(v)) if *v != user_id => continue,
            Some(Scope::In(vs)) if !vs.contains(&user_id) => continue,
            _ => {}
        }
        for entry in entries {
            // Filter by provider_id
            match (&query.provider_id, &entry.provider_id) {
                (Some(Scope::Eq(v)), Some(pid)) if v != pid => continue,
                (Some(Scope::Eq(_)), None) => continue,
                (Some(Scope::In(vs)), Some(pid)) if !vs.contains(pid) => continue,
                (Some(Scope::In(_)), None) => continue,
                _ => {}
            }
            rows.push(MemoryPermissionRow {
                id: entry.id,
                user_id,
                provider_id: entry.provider_id,
                model_pattern: entry.model_pattern.clone(),
            });
        }
    }
    rows.sort_by_key(|row| row.id);
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }
    Ok(Json(rows))
}

pub async fn upsert_permission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<gproxy_storage::UserModelPermissionWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let (payload, delete_id) = canonicalize_permission_write(&state, &payload);

    if let Some(delete_id) = delete_id {
        state.storage().delete_user_permission(delete_id).await?;
        state.remove_permission_from_memory(delete_id);
    }

    state
        .storage()
        .upsert_user_permission(payload.clone())
        .await?;

    state.upsert_permission_in_memory(
        payload.user_id,
        PermissionEntry {
            id: payload.id,
            provider_id: payload.provider_id,
            model_pattern: payload.model_pattern.clone(),
        },
    );
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeletePermissionPayload {
    pub id: i64,
}

pub async fn delete_permission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeletePermissionPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state.storage().delete_user_permission(payload.id).await?;

    state.remove_permission_from_memory(payload.id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_permissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<gproxy_storage::UserModelPermissionWrite>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for item in items {
        let (item, delete_id) = canonicalize_permission_write(&state, &item);
        if let Some(delete_id) = delete_id {
            state.storage().delete_user_permission(delete_id).await?;
            state.remove_permission_from_memory(delete_id);
        }
        state.storage().upsert_user_permission(item.clone()).await?;
        state.upsert_permission_in_memory(
            item.user_id,
            PermissionEntry {
                id: item.id,
                provider_id: item.provider_id,
                model_pattern: item.model_pattern.clone(),
            },
        );
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_permissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payloads): Json<Vec<DeletePermissionPayload>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for p in payloads {
        state.storage().delete_user_permission(p.id).await?;
        state.remove_permission_from_memory(p.id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}
