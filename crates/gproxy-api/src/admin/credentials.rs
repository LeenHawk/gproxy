use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::AppState;
use gproxy_storage::repository::CredentialRepository;
use gproxy_storage::{ProviderQuery, ProviderQueryRow, Scope};
use serde::Serialize;
use std::sync::Arc;

/// Look up a provider row from the DB by name.
async fn resolve_provider_by_name(
    state: &AppState,
    provider_name: &str,
) -> Result<ProviderQueryRow, HttpError> {
    let rows = state
        .storage()
        .list_providers(&ProviderQuery {
            name: Scope::Eq(provider_name.to_string()),
            ..Default::default()
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    rows.into_iter()
        .next()
        .ok_or_else(|| HttpError::not_found(format!("provider '{provider_name}' not found")))
}

/// Look up the DB id of a credential given its provider_id and positional index.
fn resolve_credential_db_id(
    state: &AppState,
    provider_name: &str,
    index: usize,
) -> Result<i64, HttpError> {
    state
        .credential_id_for_index(provider_name, index)
        .ok_or_else(|| HttpError::not_found("provider or credential index not found"))
}

#[derive(serde::Deserialize, Default)]
pub struct CredentialQueryParams {
    #[serde(default)]
    pub provider_name: Scope<String>,
}

#[derive(Serialize)]
pub struct CredentialRow {
    pub id: i64,
    pub provider: String,
    pub index: usize,
    pub credential: serde_json::Value,
}

/// Query credentials from SDK engine memory.
pub async fn query_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<CredentialQueryParams>,
) -> Result<Json<Vec<CredentialRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let provider_name = match &query.provider_name {
        Scope::Eq(v) => Some(v.as_str()),
        _ => None,
    };
    let creds = state
        .engine()
        .store()
        .list_credentials(provider_name)
        .map_err(|e| HttpError::internal(e.to_string()))?;
    let rows = creds
        .into_iter()
        .map(|c| {
            let credential_id = state
                .credential_id_for_index(&c.provider, c.index)
                .ok_or_else(|| {
                    HttpError::internal(format!(
                        "credential id missing for provider '{}' index {}",
                        c.provider, c.index
                    ))
                })?;
            Ok(CredentialRow {
                id: credential_id,
                provider: c.provider,
                index: c.index,
                // This is an admin/operator surface. Return the raw channel
                // credential so the management UI can inspect and edit it.
                credential: c.credential,
            })
        })
        .collect::<Result<Vec<_>, HttpError>>()?;
    Ok(Json(rows))
}

#[derive(serde::Deserialize)]
pub struct UpsertCredentialPayload {
    pub provider_name: String,
    pub credential: serde_json::Value,
}

fn validate_credential_payload(
    provider: &ProviderQueryRow,
    credential: &serde_json::Value,
) -> Result<(), HttpError> {
    gproxy_sdk::provider::engine::validate_credential_json(&provider.channel, credential).map_err(
        |err| {
            HttpError::bad_request(format!(
                "invalid credential for provider '{}': {err}",
                provider.name
            ))
        },
    )
}

async fn create_credential_and_sync_runtime(
    state: &AppState,
    provider: &ProviderQueryRow,
    credential: serde_json::Value,
) -> Result<i64, HttpError> {
    // Give the channel a chance to run any upsert-time bootstrap IO
    // (e.g. claudecode exchanges a `sessionKey` cookie for OAuth tokens
    // here so the first user request doesn't have to pay the
    // cookie→token round-trip). If the channel asks to replace the
    // credential with an updated version, persist the updated one;
    // otherwise keep the caller's original JSON. Bootstrap failures
    // propagate as `400 Bad Request` so the operator sees the real
    // cause immediately.
    let credential = match state
        .engine()
        .bootstrap_credential_on_upsert(&provider.channel, &credential)
        .await
    {
        Ok(Some(updated)) => updated,
        Ok(None) => credential,
        Err(err) => {
            return Err(HttpError::bad_request(format!(
                "credential bootstrap for provider '{}' failed: {err}",
                provider.name
            )));
        }
    };

    let credential_json = credential.to_string();
    let id = state
        .storage()
        .create_credential(provider.id, None, &provider.channel, &credential_json, true)
        .await?;

    let store = state.engine().store().clone();
    if let Some(snapshot) = store
        .add_credential(&provider.name, credential.clone())
        .map_err(|e| HttpError::internal(e.to_string()))?
    {
        state.append_provider_credential_id_in_memory(&provider.name, id);
        let _ = snapshot;
        return Ok(id);
    }

    state.storage().delete_credential(id).await?;
    Err(HttpError::not_found(format!(
        "provider '{}' not found",
        provider.name
    )))
}

/// Add or update a credential in SDK engine memory + persist to DB.
pub async fn upsert_credential(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UpsertCredentialPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let provider = resolve_provider_by_name(&state, &payload.provider_name).await?;
    validate_credential_payload(&provider, &payload.credential)?;
    create_credential_and_sync_runtime(&state, &provider, payload.credential).await?;
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeleteCredentialPayload {
    pub provider_name: String,
    pub index: usize,
}

/// Remove a credential from SDK engine memory + persist to DB.
pub async fn delete_credential(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteCredentialPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let provider = resolve_provider_by_name(&state, &payload.provider_name).await?;
    let cred_id = resolve_credential_db_id(&state, &provider.name, payload.index)?;
    state.storage().delete_credential(cred_id).await?;
    state
        .engine()
        .store()
        .remove_credential(&payload.provider_name, payload.index)
        .map_err(|e| HttpError::internal(e.to_string()))?;
    state.remove_provider_credential_index_in_memory(&payload.provider_name, payload.index);
    state.remove_user_files_for_credential(cred_id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<UpsertCredentialPayload>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let mut validated = Vec::with_capacity(items.len());
    for item in items {
        let provider = resolve_provider_by_name(&state, &item.provider_name).await?;
        validate_credential_payload(&provider, &item.credential)?;
        validated.push((provider, item.credential));
    }
    for (provider, credential) in validated {
        create_credential_and_sync_runtime(&state, &provider, credential).await?;
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<DeleteCredentialPayload>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let engine = state.engine();
    let store = engine.store();
    // Delete in reverse index order to avoid index shifting
    let mut sorted = items;
    sorted.sort_by(|a, b| b.index.cmp(&a.index));
    for item in &sorted {
        let provider = resolve_provider_by_name(&state, &item.provider_name).await?;
        let cred_id = resolve_credential_db_id(&state, &provider.name, item.index)?;
        state.storage().delete_credential(cred_id).await?;
        let _ = store.remove_credential(&item.provider_name, item.index);
        state.remove_provider_credential_index_in_memory(&item.provider_name, item.index);
        state.remove_user_files_for_credential(cred_id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize, Default)]
pub struct HealthQueryParams {
    #[serde(default)]
    pub provider_name: Scope<String>,
}

/// Query credential health from SDK engine memory.
pub async fn query_credential_statuses(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<HealthQueryParams>,
) -> Result<Json<Vec<gproxy_sdk::provider::store::CredentialHealthSnapshot>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let provider_name = match &query.provider_name {
        Scope::Eq(v) => Some(v.as_str()),
        _ => None,
    };
    let snapshots = state.engine().store().list_health(provider_name);
    Ok(Json(snapshots))
}

#[derive(serde::Deserialize)]
pub struct UpdateCredentialStatusPayload {
    pub provider_name: String,
    pub index: usize,
    /// `"healthy"` or `"dead"`.
    pub status: String,
}

/// Manually set credential health status (admin override).
pub async fn update_credential_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UpdateCredentialStatusPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let provider = resolve_provider_by_name(&state, &payload.provider_name).await?;
    let credential_id = resolve_credential_db_id(&state, &provider.name, payload.index)?;
    let engine = state.engine();
    let store = engine.store();
    if !matches!(payload.status.as_str(), "dead" | "healthy") {
        return Err(HttpError::bad_request("status must be 'healthy' or 'dead'"));
    }
    if store
        .get_credential(&payload.provider_name, payload.index)
        .map_err(|e| HttpError::internal(e.to_string()))?
        .is_none()
    {
        return Err(HttpError::not_found(
            "provider or credential index not found",
        ));
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let health_kind = payload.status.clone();
    state
        .storage()
        .upsert_credential_status(gproxy_storage::CredentialStatusWrite {
            id: None,
            credential_id,
            channel: provider.channel,
            health_kind,
            health_json: None,
            checked_at_unix_ms: Some(now_ms),
            last_error: None,
        })
        .await?;
    match payload.status.as_str() {
        "dead" => {
            store.mark_credential_dead(&payload.provider_name, payload.index);
        }
        "healthy" => {
            store.mark_credential_healthy(&payload.provider_name, payload.index);
        }
        _ => unreachable!(),
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{Json, extract::State, http::HeaderMap};

    use super::{CredentialQueryParams, query_credentials};
    use gproxy_server::{AppState, AppStateBuilder, GlobalConfig};
    use gproxy_storage::{Scope, SeaOrmStorage, repository::UserRepository};

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
        let provider_id = storage
            .create_provider(
                "demo",
                "openai",
                "{\"base_url\":\"https://api.openai.com\"}",
                "{}",
            )
            .await
            .expect("seed provider");
        storage
            .create_credential(
                provider_id,
                None,
                "openai",
                "{\"api_key\":\"sk-secret-1234\",\"label\":\"primary\"}",
                true,
            )
            .await
            .expect("seed credential");

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
    async fn query_credentials_returns_unmasked_admin_credentials() {
        let state = build_test_state().await;

        let rows = query_credentials(
            State(state),
            admin_headers(),
            Json(CredentialQueryParams {
                provider_name: Scope::Eq("demo".to_string()),
            }),
        )
        .await
        .expect("query credentials")
        .0;

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].credential["api_key"], "sk-secret-1234");
    }
}
