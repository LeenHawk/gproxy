use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::{AppState, FilePermissionEntry};
use gproxy_storage::Scope;
use gproxy_storage::repository::PermissionRepository;
use std::sync::Arc;

#[derive(serde::Serialize)]
pub struct MemoryFilePermissionRow {
    pub id: i64,
    pub user_id: i64,
    pub provider_id: i64,
}

#[derive(serde::Deserialize, Default)]
pub struct FilePermissionQueryParams {
    pub user_id: Option<Scope<i64>>,
    pub provider_id: Option<Scope<i64>>,
    pub limit: Option<usize>,
}

fn existing_file_permission_by_id(
    state: &AppState,
    permission_id: i64,
) -> Option<(i64, FilePermissionEntry)> {
    state
        .user_file_permissions_snapshot()
        .iter()
        .find_map(|(user_id, entries)| {
            entries
                .iter()
                .find(|entry| entry.id == permission_id)
                .cloned()
                .map(|entry| (*user_id, entry))
        })
}

fn existing_file_permission_by_key(
    state: &AppState,
    user_id: i64,
    provider_id: i64,
) -> Option<FilePermissionEntry> {
    state
        .user_file_permissions_snapshot()
        .get(&user_id)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.provider_id == provider_id)
                .cloned()
        })
}

fn canonicalize_file_permission_write(
    state: &AppState,
    payload: &gproxy_storage::UserFilePermissionWrite,
) -> (gproxy_storage::UserFilePermissionWrite, Option<i64>) {
    let existing_by_id = existing_file_permission_by_id(state, payload.id);
    let existing_by_key =
        existing_file_permission_by_key(state, payload.user_id, payload.provider_id);
    let effective_id = existing_by_key
        .as_ref()
        .map(|entry| entry.id)
        .unwrap_or(payload.id);
    let delete_id = existing_by_id
        .filter(|(_, entry)| entry.id != effective_id)
        .map(|(_, entry)| entry.id);

    (
        gproxy_storage::UserFilePermissionWrite {
            id: effective_id,
            user_id: payload.user_id,
            provider_id: payload.provider_id,
        },
        delete_id,
    )
}

pub async fn query_file_permissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<FilePermissionQueryParams>,
) -> Result<Json<Vec<MemoryFilePermissionRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let perms = state.user_file_permissions_snapshot();
    let mut rows = Vec::new();
    for (&user_id, entries) in perms.iter() {
        match &query.user_id {
            Some(Scope::Eq(v)) if *v != user_id => continue,
            Some(Scope::In(vs)) if !vs.contains(&user_id) => continue,
            _ => {}
        }
        for entry in entries {
            match &query.provider_id {
                Some(Scope::Eq(v)) if *v != entry.provider_id => continue,
                Some(Scope::In(vs)) if !vs.contains(&entry.provider_id) => continue,
                _ => {}
            }
            rows.push(MemoryFilePermissionRow {
                id: entry.id,
                user_id,
                provider_id: entry.provider_id,
            });
        }
    }
    rows.sort_by_key(|row| row.id);
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }
    Ok(Json(rows))
}

pub async fn upsert_file_permission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<gproxy_storage::UserFilePermissionWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let (payload, delete_id) = canonicalize_file_permission_write(&state, &payload);

    if let Some(delete_id) = delete_id {
        state
            .storage()
            .delete_user_file_permission(delete_id)
            .await?;
        state.remove_file_permission_from_memory(delete_id);
    }

    state
        .storage()
        .upsert_user_file_permission(payload.clone())
        .await?;

    state.upsert_file_permission_in_memory(
        payload.user_id,
        FilePermissionEntry {
            id: payload.id,
            provider_id: payload.provider_id,
        },
    );
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeleteFilePermissionPayload {
    pub id: i64,
}

pub async fn delete_file_permission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteFilePermissionPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state
        .storage()
        .delete_user_file_permission(payload.id)
        .await?;
    state.remove_file_permission_from_memory(payload.id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_file_permissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<gproxy_storage::UserFilePermissionWrite>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for item in items {
        let (item, delete_id) = canonicalize_file_permission_write(&state, &item);
        if let Some(delete_id) = delete_id {
            state
                .storage()
                .delete_user_file_permission(delete_id)
                .await?;
            state.remove_file_permission_from_memory(delete_id);
        }
        state
            .storage()
            .upsert_user_file_permission(item.clone())
            .await?;
        state.upsert_file_permission_in_memory(
            item.user_id,
            FilePermissionEntry {
                id: item.id,
                provider_id: item.provider_id,
            },
        );
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_file_permissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payloads): Json<Vec<DeleteFilePermissionPayload>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for payload in payloads {
        state
            .storage()
            .delete_user_file_permission(payload.id)
            .await?;
        state.remove_file_permission_from_memory(payload.id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}
