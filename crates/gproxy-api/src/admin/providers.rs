use crate::auth::authorize_admin;
use crate::bootstrap::{
    apply_persisted_credential_statuses, collect_valid_db_provider_credentials,
    ensure_default_models_in_storage, model_rows_to_memory_models,
};
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_sdk::provider::dispatch::DispatchTableDocument;
use gproxy_sdk::provider::engine::{GproxyEngineBuilder, ProviderConfig};
use gproxy_sdk::provider::registry::ChannelRegistry;
use gproxy_server::AppState;
use gproxy_storage::repository::ProviderRepository;
use gproxy_storage::{CredentialQuery, ProviderQuery, ProviderQueryRow, Scope};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Look up a provider's DB id by name.
async fn resolve_provider_id_by_name(state: &AppState, name: &str) -> Result<i64, HttpError> {
    let rows = state
        .storage()
        .list_providers(&ProviderQuery {
            name: Scope::Eq(name.to_string()),
            ..Default::default()
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    rows.into_iter()
        .next()
        .map(|r| r.id)
        .ok_or_else(|| HttpError::not_found(format!("provider '{name}' not found in DB")))
}

async fn load_providers_by_id(
    state: &AppState,
) -> Result<HashMap<i64, ProviderQueryRow>, HttpError> {
    let rows = state
        .storage()
        .list_providers(&ProviderQuery::default())
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(rows.into_iter().map(|row| (row.id, row)).collect())
}

fn parse_dispatch_document_json(
    dispatch_json: &str,
) -> Result<Option<DispatchTableDocument>, HttpError> {
    let trimmed = dispatch_json.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|e| HttpError::bad_request(format!("invalid provider dispatch_json: {e}")))?;
    DispatchTableDocument::from_json_value(value)
        .map_err(|e| HttpError::bad_request(format!("invalid provider dispatch_json: {e}")))
}

fn default_dispatch_document_for_channel(
    channel: &str,
) -> Result<DispatchTableDocument, HttpError> {
    ChannelRegistry::collect()
        .dispatch_table(channel)
        .map(|table| table.to_document())
        .ok_or_else(|| HttpError::bad_request(format!("unknown provider channel: {channel}")))
}

async fn sync_provider_runtime(
    state: &AppState,
    payload: &gproxy_storage::ProviderWrite,
    previous_name: Option<&str>,
) -> Result<(), HttpError> {
    let store = state.engine().store().clone();
    let previous_runtime_name = if let Some(old_name) = previous_name {
        if store
            .get_provider(old_name)
            .map_err(|e| HttpError::internal(e.to_string()))?
            .is_some()
        {
            Some(old_name.to_string())
        } else {
            None
        }
    } else {
        None
    };

    if let Some(old_name) = previous_name
        && old_name != payload.name
    {
        store.remove_provider(old_name);
        state.remove_provider_name_from_memory(old_name);
        state.remove_provider_channel_from_memory(old_name);
        state.remove_provider_label_from_memory(old_name);
        state.remove_provider_credentials_from_memory(old_name);
    }

    state.upsert_provider_name_in_memory(payload.name.clone(), payload.id);
    state.upsert_provider_channel_in_memory(payload.name.clone(), payload.channel.clone());
    state.upsert_provider_label_in_memory(payload.name.clone(), payload.label.clone());
    let dispatch = parse_dispatch_document_json(&payload.dispatch_json)?;

    store.remove_provider(&payload.name);

    let credentials = state
        .storage()
        .list_credentials(&CredentialQuery {
            provider_id: Scope::Eq(payload.id),
            enabled: Scope::Eq(true),
            ..Default::default()
        })
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    let valid_db_credentials =
        collect_valid_db_provider_credentials(&payload.name, &payload.channel, &credentials);
    let runtime_credentials = previous_runtime_name
        .as_deref()
        .or(Some(payload.name.as_str()))
        .and_then(|provider_name| store.list_credentials(Some(provider_name)).ok())
        .filter(|creds| !creds.is_empty())
        .map(|creds| {
            creds
                .into_iter()
                .map(|cred| cred.credential)
                .collect::<Vec<_>>()
        });
    let credential_ids = if runtime_credentials.is_some() {
        state
            .provider_credential_ids_for(previous_runtime_name.as_deref().unwrap_or(&payload.name))
            .unwrap_or_else(|| {
                valid_db_credentials
                    .iter()
                    .map(|(credential_id, _)| *credential_id)
                    .collect()
            })
    } else {
        valid_db_credentials
            .iter()
            .map(|(credential_id, _)| *credential_id)
            .collect()
    };

    let provider_config = ProviderConfig {
        name: payload.name.clone(),
        channel: payload.channel.clone(),
        settings_json: serde_json::from_str(&payload.settings_json).unwrap_or_default(),
        credentials: runtime_credentials.unwrap_or_else(|| {
            valid_db_credentials
                .iter()
                .map(|(_, credential)| credential.clone())
                .collect()
        }),
        dispatch,
    };
    store
        .add_provider_json(provider_config)
        .map_err(|e| HttpError::internal(e.to_string()))?;
    state.replace_provider_credential_ids_in_memory(payload.name.clone(), credential_ids);

    let credential_positions: HashMap<i64, (String, usize)> = valid_db_credentials
        .iter()
        .enumerate()
        .map(|(index, (credential_id, _))| (*credential_id, (payload.name.clone(), index)))
        .collect();
    apply_persisted_credential_statuses(state, &credential_positions)
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    let model_rows =
        ensure_default_models_in_storage(state, &[(payload.id, payload.channel.clone())])
            .await
            .map_err(|e| HttpError::internal(e.to_string()))?;
    state.replace_models(model_rows_to_memory_models(&model_rows));
    // Push pricing into the engine for every registered provider so admin-edited
    // prices on any provider take effect at billing time after a provider upsert.
    for snapshot in state.engine().store().list_providers().unwrap_or_default() {
        state.push_pricing_to_engine(&snapshot.name);
    }

    Ok(())
}

fn validate_provider_payload(payload: &gproxy_storage::ProviderWrite) -> Result<(), HttpError> {
    let settings_json = serde_json::from_str(&payload.settings_json)
        .map_err(|e| HttpError::bad_request(format!("invalid provider settings_json: {e}")))?;
    let dispatch = parse_dispatch_document_json(&payload.dispatch_json)?;
    GproxyEngineBuilder::new()
        .add_provider_json(ProviderConfig {
            name: payload.name.clone(),
            channel: payload.channel.clone(),
            settings_json,
            credentials: Vec::new(),
            dispatch,
        })
        .map(|_| ())
        .map_err(|e| HttpError::bad_request(e.to_string()))
}

fn ensure_provider_channel_immutable(
    existing: Option<&ProviderQueryRow>,
    payload: &gproxy_storage::ProviderWrite,
) -> Result<(), HttpError> {
    if let Some(existing) = existing
        && existing.channel != payload.channel
    {
        return Err(HttpError::bad_request(format!(
            "changing provider '{}' channel from '{}' to '{}' is not allowed",
            existing.name, existing.channel, payload.channel
        )));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ProviderRow {
    pub id: i64,
    pub name: String,
    pub channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub settings_json: serde_json::Value,
    pub dispatch_json: serde_json::Value,
    pub credential_count: usize,
}

#[derive(serde::Deserialize, Default)]
pub struct ProviderQueryParams {
    #[serde(default)]
    pub name: Scope<String>,
    #[serde(default)]
    pub channel: Scope<String>,
}

#[derive(serde::Deserialize)]
pub struct ProviderDispatchTemplateParams {
    pub channel: String,
}

/// Query providers from SDK engine memory.
pub async fn query_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<ProviderQueryParams>,
) -> Result<Json<Vec<ProviderRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let store = state.engine().store().clone();
    let snapshots = store
        .list_providers()
        .map_err(|e| HttpError::internal(e.to_string()))?;
    let rows: Vec<ProviderRow> = snapshots
        .into_iter()
        .filter(|s| match &query.name {
            Scope::Eq(v) => s.name == *v,
            _ => true,
        })
        .filter(|s| match &query.channel {
            Scope::Eq(v) => s.channel == *v,
            _ => true,
        })
        .map(|s| {
            let provider_id = state.provider_id_for_name(&s.name).ok_or_else(|| {
                HttpError::internal(format!("provider id missing for '{}'", s.name))
            })?;
            let dispatch_json = store
                .get_dispatch_table(&s.name)
                .map(|dispatch| {
                    serde_json::to_value(dispatch.to_document()).map_err(|e| {
                        HttpError::internal(format!(
                            "serialize dispatch for provider '{}': {e}",
                            s.name
                        ))
                    })
                })
                .transpose()?
                .unwrap_or_else(|| serde_json::json!({ "rules": [] }));
            Ok(ProviderRow {
                id: provider_id,
                name: s.name.clone(),
                channel: s.channel,
                label: state.provider_label_for_name(&s.name),
                settings_json: s.settings,
                dispatch_json,
                credential_count: s.credential_count,
            })
        })
        .collect::<Result<Vec<_>, HttpError>>()?;
    Ok(Json(rows))
}

pub async fn default_provider_dispatch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<ProviderDispatchTemplateParams>,
) -> Result<Json<DispatchTableDocument>, HttpError> {
    authorize_admin(&headers, &state)?;
    Ok(Json(default_dispatch_document_for_channel(
        payload.channel.trim(),
    )?))
}

/// Upsert provider — persists to DB.
/// Note: provider changes in the SDK engine require rebuild (takes effect on restart).
pub async fn upsert_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<gproxy_storage::ProviderWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let existing = load_providers_by_id(&state).await?;
    ensure_provider_channel_immutable(existing.get(&payload.id), &payload)?;
    validate_provider_payload(&payload)?;
    let previous_name = existing.get(&payload.id).map(|row| row.name.clone());
    state.storage().upsert_provider(payload.clone()).await?;
    sync_provider_runtime(&state, &payload, previous_name.as_deref()).await?;
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeleteProviderPayload {
    pub name: String,
}

/// Delete provider — persists to DB and removes from SDK engine memory.
pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteProviderPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let provider_id = resolve_provider_id_by_name(&state, &payload.name).await?;
    state.storage().delete_provider(provider_id).await?;
    state.engine().store().remove_provider(&payload.name);
    state.remove_provider_name_from_memory(&payload.name);
    state.remove_provider_channel_from_memory(&payload.name);
    state.remove_provider_label_from_memory(&payload.name);
    state.remove_provider_credentials_from_memory(&payload.name);
    state.remove_file_permissions_for_provider(provider_id);
    state.remove_user_files_for_provider(provider_id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<gproxy_storage::ProviderWrite>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let existing = load_providers_by_id(&state).await?;
    for item in &items {
        ensure_provider_channel_immutable(existing.get(&item.id), item)?;
        validate_provider_payload(item)?;
    }
    for item in items {
        let previous_name = existing.get(&item.id).map(|row| row.name.as_str());
        state.storage().upsert_provider(item.clone()).await?;
        sync_provider_runtime(&state, &item, previous_name).await?;
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(names): Json<Vec<String>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for name in &names {
        let provider_id = resolve_provider_id_by_name(&state, name).await?;
        state.storage().delete_provider(provider_id).await?;
        state.engine().store().remove_provider(name);
        state.remove_provider_name_from_memory(name);
        state.remove_provider_channel_from_memory(name);
        state.remove_provider_label_from_memory(name);
        state.remove_provider_credentials_from_memory(name);
        state.remove_file_permissions_for_provider(provider_id);
        state.remove_user_files_for_provider(provider_id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{Json, extract::State, http::HeaderMap};
    use gproxy_sdk::protocol::kinds::{OperationFamily, ProtocolKind};
    use gproxy_sdk::provider::dispatch::{RouteImplementation, RouteKey};
    use time::OffsetDateTime;

    use super::{
        ProviderDispatchTemplateParams, ProviderQueryParams, default_provider_dispatch,
        ensure_provider_channel_immutable, query_providers, upsert_provider,
    };
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
        assert!(provider_id > 0);

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

    #[test]
    fn provider_channel_is_immutable_once_created() {
        let existing = gproxy_storage::ProviderQueryRow {
            id: 1,
            name: "demo".to_string(),
            channel: "openai".to_string(),
            label: None,
            settings_json: serde_json::json!({}),
            dispatch_json: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        let payload = gproxy_storage::ProviderWrite {
            id: 1,
            name: "demo".to_string(),
            channel: "anthropic".to_string(),
            label: None,
            settings_json: "{}".to_string(),
            dispatch_json: "{}".to_string(),
        };

        let err = ensure_provider_channel_immutable(Some(&existing), &payload).unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn provider_channel_validation_allows_same_channel() {
        let existing = gproxy_storage::ProviderQueryRow {
            id: 1,
            name: "demo".to_string(),
            channel: "openai".to_string(),
            label: None,
            settings_json: serde_json::json!({}),
            dispatch_json: serde_json::json!({}),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        let payload = gproxy_storage::ProviderWrite {
            id: 1,
            name: "demo-renamed".to_string(),
            channel: "openai".to_string(),
            label: None,
            settings_json: "{}".to_string(),
            dispatch_json: "{}".to_string(),
        };

        assert!(ensure_provider_channel_immutable(Some(&existing), &payload).is_ok());
    }

    #[tokio::test]
    async fn query_providers_includes_provider_id() {
        let state = build_test_state().await;

        let rows = query_providers(
            State(state),
            admin_headers(),
            Json(ProviderQueryParams {
                name: Scope::Eq("demo".to_string()),
                channel: Scope::All,
            }),
        )
        .await
        .expect("query providers")
        .0;

        assert_eq!(rows.len(), 1);
        let json = serde_json::to_value(&rows[0]).expect("serialize row");
        assert!(
            json["id"].as_i64().is_some(),
            "provider row should include id"
        );
        assert!(
            json["dispatch_json"]["rules"].as_array().is_some(),
            "provider row should expose canonical dispatch rules"
        );
    }

    #[tokio::test]
    async fn default_provider_dispatch_returns_channel_template() {
        let state = build_test_state().await;

        let document = default_provider_dispatch(
            State(state),
            admin_headers(),
            Json(ProviderDispatchTemplateParams {
                channel: "openai".to_string(),
            }),
        )
        .await
        .expect("default dispatch")
        .0;

        assert!(
            !document.rules.is_empty(),
            "default dispatch should expose at least one route"
        );
    }

    #[tokio::test]
    async fn upsert_provider_updates_runtime_dispatch_immediately() {
        let state = build_test_state().await;

        let _ = upsert_provider(
            State(state.clone()),
            admin_headers(),
            Json(gproxy_storage::ProviderWrite {
                id: 1,
                name: "demo".to_string(),
                channel: "openai".to_string(),
                label: None,
                settings_json: "{\"base_url\":\"https://api.openai.com\"}".to_string(),
                dispatch_json: serde_json::json!({
                    "rules": [
                        {
                            "route": {
                                "operation": "generate_content",
                                "protocol": "openai"
                            },
                            "implementation": "Unsupported"
                        }
                    ]
                })
                .to_string(),
            }),
        )
        .await
        .expect("upsert provider");

        let dispatch = state
            .engine()
            .store()
            .get_dispatch_table("demo")
            .expect("runtime dispatch table");
        let implementation = dispatch
            .resolve(&RouteKey::new(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAi,
            ))
            .expect("generate_content route");

        assert_eq!(*implementation, RouteImplementation::Unsupported);
        let models = state
            .storage()
            .list_models(&gproxy_storage::ModelQuery {
                provider_id: Scope::Eq(1),
                ..Default::default()
            })
            .await
            .expect("query models");
        assert!(!models.is_empty(), "provider should seed default models");
    }
}
