use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_sdk::provider::engine::{ExecuteBody, ExecuteRequest};
use gproxy_server::{
    AppState, MemoryModel, OperationFamily, PriceTier, ProtocolKind,
};
use gproxy_storage::Scope;
use gproxy_storage::repository::ModelRepository;
use std::collections::HashMap;
use std::sync::Arc;

/// Resolve a single provider_id to its name via storage query.
async fn resolve_provider_name(state: &AppState, provider_id: i64) -> Result<String, HttpError> {
    let storage = state.storage();
    let providers = storage
        .list_providers(&gproxy_storage::ProviderQuery::default())
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    providers
        .iter()
        .find(|p| p.id == provider_id)
        .map(|p| p.name.clone())
        .ok_or_else(|| HttpError::internal(format!("provider_id {} not found", provider_id)))
}

/// Build a provider_id -> name map for a set of provider IDs.
async fn resolve_provider_names(state: &AppState) -> Result<HashMap<i64, String>, HttpError> {
    let storage = state.storage();
    let providers = storage
        .list_providers(&gproxy_storage::ProviderQuery::default())
        .await
        .map_err(|e| HttpError::internal(e.to_string()))?;
    Ok(providers.into_iter().map(|p| (p.id, p.name)).collect())
}

/// Response row for query_models (from in-memory data, no timestamps).
#[derive(serde::Serialize)]
pub struct MemoryModelRow {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    pub price_each_call: Option<f64>,
    pub price_tiers: Vec<PriceTier>,
}

/// Query filter for models (simplified from storage ModelQuery).
#[derive(serde::Deserialize, Default)]
pub struct ModelQueryParams {
    pub id: Option<Scope<i64>>,
    pub provider_id: Option<Scope<i64>>,
    pub model_id: Option<Scope<String>>,
    pub enabled: Option<Scope<bool>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

fn scope_matches<T: PartialEq>(scope: &Option<Scope<T>>, value: &T) -> bool {
    match scope {
        None => true,
        Some(Scope::All) => true,
        Some(Scope::Eq(v)) => v == value,
        Some(Scope::In(vs)) => vs.contains(value),
    }
}

pub async fn query_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<ModelQueryParams>,
) -> Result<Json<Vec<MemoryModelRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let models = state.models();
    let mut rows: Vec<MemoryModelRow> = models
        .iter()
        .filter(|m| {
            scope_matches(&query.id, &m.id)
                && scope_matches(&query.provider_id, &m.provider_id)
                && scope_matches(&query.model_id, &m.model_id)
                && scope_matches(&query.enabled, &m.enabled)
        })
        .map(|m| MemoryModelRow {
            id: m.id,
            provider_id: m.provider_id,
            model_id: m.model_id.clone(),
            display_name: m.display_name.clone(),
            enabled: m.enabled,
            price_each_call: m.price_each_call,
            price_tiers: m.price_tiers.clone(),
        })
        .collect();

    let offset = query.offset.unwrap_or(0);
    if offset > 0 {
        rows = rows.into_iter().skip(offset).collect();
    }
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }
    Ok(Json(rows))
}

pub async fn upsert_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<gproxy_storage::ModelWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    state.storage().upsert_model(payload.clone()).await?;

    let price_tiers: Vec<PriceTier> = payload
        .price_tiers_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    state.upsert_model_in_memory(MemoryModel {
        id: payload.id,
        provider_id: payload.provider_id,
        model_id: payload.model_id.clone(),
        display_name: payload.display_name.clone(),
        enabled: payload.enabled,
        price_each_call: payload.price_each_call,
        price_tiers,
        alias_of: payload.alias_of,
    });
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeleteModelPayload {
    id: i64,
}

pub async fn delete_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteModelPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    state.storage().delete_model(payload.id).await?;

    state.remove_model_from_memory(payload.id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

/// Response row for model aliases from memory.
#[derive(serde::Serialize)]
pub struct MemoryModelAliasRow {
    pub id: i64,
    pub alias: String,
    pub provider_name: String,
    pub model_id: String,
}

pub async fn query_model_aliases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<MemoryModelAliasRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let provider_names = resolve_provider_names(&state).await?;
    let models = state.models();
    let rows: Vec<MemoryModelAliasRow> = models
        .iter()
        .filter(|m| m.alias_of.is_some())
        .filter_map(|m| {
            // Find the target model to get its model_id
            let target_id = m.alias_of?;
            let target = models.iter().find(|t| t.id == target_id)?;
            Some(MemoryModelAliasRow {
                id: m.id,
                alias: m.model_id.clone(),
                provider_name: provider_names
                    .get(&target.provider_id)
                    .cloned()
                    .unwrap_or_else(|| target.provider_id.to_string()),
                model_id: target.model_id.clone(),
            })
        })
        .collect();
    Ok(Json(rows))
}

/// Payload for upserting a model alias (now stored as a model row with alias_of).
#[derive(serde::Deserialize, Clone)]
pub struct UpsertModelAliasPayload {
    pub id: i64,
    pub alias: String,
    pub provider_id: i64,
    pub model_id: String,
    pub enabled: bool,
}

pub async fn upsert_model_alias(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UpsertModelAliasPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    // Find the target model by provider_id + model_id
    let models = state.models();
    let target = models
        .iter()
        .find(|m| m.provider_id == payload.provider_id && m.model_id == payload.model_id && m.alias_of.is_none());
    let alias_of = target.map(|t| t.id);

    state
        .storage()
        .upsert_model(gproxy_storage::ModelWrite {
            id: payload.id,
            provider_id: payload.provider_id,
            model_id: payload.alias.clone(),
            display_name: None,
            enabled: payload.enabled,
            price_each_call: None,
            price_tiers_json: None,
            alias_of,
        })
        .await?;

    state.upsert_model_in_memory(MemoryModel {
        id: payload.id,
        provider_id: payload.provider_id,
        model_id: payload.alias,
        display_name: None,
        enabled: payload.enabled,
        price_each_call: None,
        price_tiers: Vec::new(),
        alias_of,
    });
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[derive(serde::Deserialize)]
pub struct DeleteModelAliasPayload {
    alias: String,
}

pub async fn delete_model_alias(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteModelAliasPayload>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    // Find the alias model by model_id (alias name) with alias_of set
    let models = state.models();
    let alias_model = models
        .iter()
        .find(|m| m.model_id == payload.alias && m.alias_of.is_some())
        .ok_or_else(|| HttpError::not_found("model alias not found"))?;
    let id = alias_model.id;

    state.storage().delete_model(id).await?;
    state.remove_model_from_memory(id);
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<gproxy_storage::ModelWrite>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for item in items {
        state.storage().upsert_model(item.clone()).await?;
        let price_tiers: Vec<PriceTier> = item
            .price_tiers_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        state.upsert_model_in_memory(MemoryModel {
            id: item.id,
            provider_id: item.provider_id,
            model_id: item.model_id.clone(),
            display_name: item.display_name.clone(),
            enabled: item.enabled,
            price_each_call: item.price_each_call,
            price_tiers,
            alias_of: item.alias_of,
        });
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for id in ids {
        state.storage().delete_model(id).await?;
        state.remove_model_from_memory(id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_model_aliases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<UpsertModelAliasPayload>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    for item in items {
        let models = state.models();
        let target = models
            .iter()
            .find(|m| m.provider_id == item.provider_id && m.model_id == item.model_id && m.alias_of.is_none());
        let alias_of = target.map(|t| t.id);

        state
            .storage()
            .upsert_model(gproxy_storage::ModelWrite {
                id: item.id,
                provider_id: item.provider_id,
                model_id: item.alias.clone(),
                display_name: None,
                enabled: item.enabled,
                price_each_call: None,
                price_tiers_json: None,
                alias_of,
            })
            .await?;

        state.upsert_model_in_memory(MemoryModel {
            id: item.id,
            provider_id: item.provider_id,
            model_id: item.alias,
            display_name: None,
            enabled: item.enabled,
            price_each_call: None,
            price_tiers: Vec::new(),
            alias_of,
        });
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_model_aliases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payloads): Json<Vec<DeleteModelAliasPayload>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    for p in payloads {
        let models = state.models();
        let alias_model = models
            .iter()
            .find(|m| m.model_id == p.alias && m.alias_of.is_some())
            .ok_or_else(|| HttpError::not_found("model alias not found"))?;
        let id = alias_model.id;
        state.storage().delete_model(id).await?;
        state.remove_model_from_memory(id);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

// ---------------------------------------------------------------------------
// Pull live model list from a provider
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct PullModelsPayload {
    pub provider_id: i64,
}

#[derive(serde::Serialize)]
pub struct PullModelsResponse {
    pub models: Vec<String>,
}

/// Determine the protocol to use for model_list based on channel name.
fn channel_to_model_list_protocol(channel: &str) -> ProtocolKind {
    match channel {
        "anthropic" | "claudecode" => ProtocolKind::Claude,
        "vertex" | "vertexexpress" | "aistudio" | "geminicli" => ProtocolKind::Gemini,
        _ => ProtocolKind::OpenAi,
    }
}

/// Build the request body for a live model_list call (mirrors handler.rs logic).
fn build_live_model_list_request_body(protocol: ProtocolKind) -> Vec<u8> {
    match protocol {
        ProtocolKind::Claude => serde_json::to_vec(&serde_json::json!({
            "query": { "limit": 1000 }
        }))
        .unwrap_or_default(),
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            serde_json::to_vec(&serde_json::json!({
                "query": { "pageSize": 1000 }
            }))
            .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// Extract model IDs from the response body, adapting to the protocol's format.
fn extract_model_ids(body: &[u8], protocol: ProtocolKind) -> Vec<String> {
    match protocol {
        ProtocolKind::Claude => {
            // Claude: { "data": [{ "id": "..." }, ...] }
            if let Ok(resp) = serde_json::from_slice::<serde_json::Value>(body) {
                resp.get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            // Gemini: { "models": [{ "name": "models/gemini-pro", ... }] }
            if let Ok(resp) = serde_json::from_slice::<serde_json::Value>(body) {
                resp.get("models")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                m.get("name").and_then(|v| v.as_str()).map(|name| {
                                    name.strip_prefix("models/").unwrap_or(name).to_string()
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        _ => {
            // OpenAI: { "data": [{ "id": "gpt-4o", ... }] }
            if let Ok(resp) = serde_json::from_slice::<serde_json::Value>(body) {
                resp.get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
    }
}

pub async fn pull_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<PullModelsPayload>,
) -> Result<Json<PullModelsResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    // Resolve provider_id -> provider_name
    let provider_name = resolve_provider_name(&state, payload.provider_id).await?;

    // Determine protocol from channel
    let channel = state
        .provider_channel_for_name(&provider_name)
        .ok_or_else(|| {
            HttpError::internal(format!(
                "provider '{}' has no channel configured",
                provider_name
            ))
        })?;
    let protocol = channel_to_model_list_protocol(&channel);

    // Execute live model list request via the engine
    let result = state
        .engine()
        .execute(ExecuteRequest {
            provider: provider_name.clone(),
            operation: OperationFamily::ModelList,
            protocol,
            body: build_live_model_list_request_body(protocol),
            headers: headers.clone(),
            model: None,
            forced_credential_index: None,
            response_model_override: None,
        })
        .await
        .map_err(|e| HttpError::internal(format!("engine execute failed: {e}")))?;

    if !(200..=299).contains(&result.status) {
        return Err(HttpError::internal(format!(
            "provider '{}' model list failed with HTTP {}",
            provider_name, result.status
        )));
    }

    let ExecuteBody::Full(body) = result.body else {
        return Err(HttpError::internal(
            "provider returned streaming response for model list".to_string(),
        ));
    };

    let mut models = extract_model_ids(&body, protocol);
    models.sort();
    models.dedup();

    Ok(Json(PullModelsResponse { models }))
}
