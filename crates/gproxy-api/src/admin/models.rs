use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_sdk::provider::engine::{ExecuteBody, ExecuteRequest};
use gproxy_server::{AppState, MemoryModel, OperationFamily, ProtocolKind};
use gproxy_storage::Scope;
use gproxy_storage::repository::ModelRepository;
use std::collections::BTreeSet;
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

/// Response row for query_models (from in-memory data, no timestamps).
#[derive(serde::Serialize)]
pub struct MemoryModelRow {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    /// Full serialized ModelPrice JSON (matches `models.pricing_json`).
    pub pricing_json: Option<String>,
    /// NULL = real model, Some(id) = alias pointing to another model's id.
    pub alias_of: Option<i64>,
}

/// Query filter for models (simplified from storage ModelQuery).
#[derive(serde::Deserialize, Default)]
pub struct ModelQueryParams {
    pub id: Option<Scope<i64>>,
    pub provider_id: Option<Scope<i64>>,
    pub model_id: Option<Scope<String>>,
    pub enabled: Option<Scope<bool>>,
    /// Filter by alias status:
    /// - omit / null → return all models (aliases + real)
    /// - `"only_aliases"` → only rows where alias_of IS NOT NULL
    /// - `"only_real"` → only rows where alias_of IS NULL
    pub alias_of_filter: Option<AliasOfFilter>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum AliasOfFilter {
    OnlyAliases,
    OnlyReal,
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
                && match query.alias_of_filter {
                    None => true,
                    Some(AliasOfFilter::OnlyAliases) => m.alias_of.is_some(),
                    Some(AliasOfFilter::OnlyReal) => m.alias_of.is_none(),
                }
        })
        .map(|m| MemoryModelRow {
            id: m.id,
            provider_id: m.provider_id,
            model_id: m.model_id.clone(),
            display_name: m.display_name.clone(),
            enabled: m.enabled,
            pricing_json: m.pricing.as_ref().and_then(|mp| {
                match crate::bootstrap::model_price_to_storage_json(mp) {
                    Ok(s) => Some(s),
                    Err(err) => {
                        tracing::warn!(
                            model_id = %m.model_id,
                            error = %err,
                            "failed to serialize ModelPrice for query_models response"
                        );
                        None
                    }
                }
            }),
            alias_of: m.alias_of,
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

    // Validate pricing_json up front so we reject malformed input before
    // writing to the DB.
    let pricing: Option<gproxy_sdk::provider::billing::ModelPrice> = payload
        .pricing_json
        .as_deref()
        .map(|raw| serde_json::from_str(raw))
        .transpose()
        .map_err(|e| HttpError::bad_request(format!("invalid pricing_json: {e}")))?
        .map(|mut mp: gproxy_sdk::provider::billing::ModelPrice| {
            mp.model_id = payload.model_id.clone();
            mp.display_name = payload.display_name.clone();
            mp
        });

    state.storage().upsert_model(payload.clone()).await?;

    state.upsert_model_in_memory(MemoryModel {
        id: payload.id,
        provider_id: payload.provider_id,
        model_id: payload.model_id.clone(),
        display_name: payload.display_name.clone(),
        enabled: payload.enabled,
        pricing,
        alias_of: payload.alias_of,
    });

    let provider_name = resolve_provider_name(&state, payload.provider_id).await?;
    state.push_pricing_to_engine(&provider_name);

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

    let provider_id_for_delete = state
        .models()
        .iter()
        .find(|m| m.id == payload.id)
        .map(|m| m.provider_id);

    state.storage().delete_model(payload.id).await?;
    state.remove_model_from_memory(payload.id);

    if let Some(pid) = provider_id_for_delete {
        let name = resolve_provider_name(&state, pid).await?;
        state.push_pricing_to_engine(&name);
    }

    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_upsert_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(items): Json<Vec<gproxy_storage::ModelWrite>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    // Pre-pass: validate every item's pricing_json before writing any of
    // them. Rejecting a batch halfway would leave the DB in a partial
    // state that's annoying to reason about.
    let parsed: Vec<Option<gproxy_sdk::provider::billing::ModelPrice>> = items
        .iter()
        .map(|item| {
            item.pricing_json
                .as_deref()
                .map(|raw| serde_json::from_str(raw))
                .transpose()
                .map_err(|e| {
                    HttpError::bad_request(format!(
                        "invalid pricing_json for model {}: {e}",
                        item.model_id
                    ))
                })
                .map(|parsed_opt| {
                    parsed_opt.map(|mut mp: gproxy_sdk::provider::billing::ModelPrice| {
                        mp.model_id = item.model_id.clone();
                        mp.display_name = item.display_name.clone();
                        mp
                    })
                })
        })
        .collect::<Result<_, _>>()?;

    for (item, pricing) in items.iter().zip(parsed.into_iter()) {
        state.storage().upsert_model(item.clone()).await?;
        state.upsert_model_in_memory(MemoryModel {
            id: item.id,
            provider_id: item.provider_id,
            model_id: item.model_id.clone(),
            display_name: item.display_name.clone(),
            enabled: item.enabled,
            pricing,
            alias_of: item.alias_of,
        });
    }
    let touched_providers: BTreeSet<i64> = items.iter().map(|i| i.provider_id).collect();
    for pid in touched_providers {
        let name = resolve_provider_name(&state, pid).await?;
        state.push_pricing_to_engine(&name);
    }
    Ok(Json(AckResponse { ok: true, id: None }))
}

pub async fn batch_delete_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(ids): Json<Vec<i64>>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    // Collect provider_ids before deleting from memory.
    let touched_providers: BTreeSet<i64> = {
        let models = state.models();
        ids.iter()
            .filter_map(|id| models.iter().find(|m| m.id == *id).map(|m| m.provider_id))
            .collect()
    };
    for id in ids {
        state.storage().delete_model(id).await?;
        state.remove_model_from_memory(id);
    }
    for pid in touched_providers {
        let name = resolve_provider_name(&state, pid).await?;
        state.push_pricing_to_engine(&name);
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

    // Execute live model list request via the engine.
    // Pass an empty HeaderMap — the admin request headers (Authorization,
    // Content-Length, Host, etc.) would leak to the upstream and break it.
    // The engine/channel finalize_request adds the provider's own auth headers.
    let result = state
        .engine()
        .execute(ExecuteRequest {
            provider: provider_name.clone(),
            operation: OperationFamily::ModelList,
            protocol,
            body: build_live_model_list_request_body(protocol),
            headers: HeaderMap::new(),
            model: None,
            forced_credential_index: None,
            response_model_override: None,
        })
        .await
        .map_err(|e| HttpError::internal(format!("engine execute failed: {e}")))?;

    if !(200..=299).contains(&result.status) {
        // Include the upstream response body so admins can see what went wrong.
        let body_preview = match &result.body {
            ExecuteBody::Full(bytes) => String::from_utf8_lossy(bytes)
                .chars()
                .take(500)
                .collect::<String>(),
            ExecuteBody::Stream(_) => "<streaming>".to_string(),
        };
        return Err(HttpError::internal(format!(
            "provider '{}' model list failed with HTTP {}: {}",
            provider_name, result.status, body_preview
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gproxy_sdk::provider::billing::{BillingContext, BillingMode};
    use gproxy_sdk::provider::engine::Usage;
    use gproxy_server::{AppState, AppStateBuilder, GlobalConfig, MemoryModel};
    use gproxy_storage::{
        SeaOrmStorage,
        repository::{ModelRepository, UserRepository},
    };

    async fn build_test_state_for_pricing() -> Arc<AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        // Seed an admin user + key so authorize_admin passes if needed.
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
        // Create an openai provider so the engine has a registered provider.
        storage
            .create_provider(
                "openai-test",
                "openai",
                "{\"base_url\":\"https://api.openai.com\"}",
                "{}",
            )
            .await
            .expect("seed provider");

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

    #[tokio::test]
    async fn admin_upsert_model_price_affects_billing() {
        let state = build_test_state_for_pricing().await;
        let provider_name = "openai-test";
        let provider_id = state
            .provider_id_for_name(provider_name)
            .expect("provider registered");
        // Use a model_id that does NOT exist in the built-in price table so that
        // without the push the engine has no entry and estimate_billing returns None.
        let model_id = "gpt-custom-pricing-test-9999";

        // Insert the model row into storage and in-memory state, then push pricing.
        let model_price = gproxy_sdk::provider::billing::ModelPrice {
            model_id: model_id.to_string(),
            display_name: None,
            price_each_call: Some(999.0),
            price_tiers: Vec::new(),
            flex_price_each_call: None,
            flex_price_tiers: Vec::new(),
            scale_price_each_call: None,
            scale_price_tiers: Vec::new(),
            priority_price_each_call: None,
            priority_price_tiers: Vec::new(),
            tool_call_prices: std::collections::BTreeMap::new(),
        };
        let pricing_json_str = serde_json::to_string(&model_price).unwrap();

        state
            .storage()
            .upsert_model(gproxy_storage::ModelWrite {
                id: 99999,
                provider_id,
                model_id: model_id.to_string(),
                display_name: None,
                enabled: true,
                price_each_call: None,
                price_tiers_json: None,
                pricing_json: Some(pricing_json_str),
                alias_of: None,
            })
            .await
            .expect("upsert model in storage");
        state.upsert_model_in_memory(MemoryModel {
            id: 99999,
            provider_id,
            model_id: model_id.to_string(),
            display_name: None,
            enabled: true,
            pricing: Some(model_price),
            alias_of: None,
        });
        state.push_pricing_to_engine(provider_name);

        let ctx = BillingContext {
            model_id: model_id.to_string(),
            mode: BillingMode::Default,
        };
        let usage = Usage::default();
        let result = state
            .engine()
            .estimate_billing(provider_name, &ctx, &usage)
            .expect("estimate_billing must return Some — push_pricing_to_engine was not called or failed");
        assert!(
            (result.total_cost - 999.0).abs() < 1e-9,
            "expected total_cost 999.0, got {}",
            result.total_cost
        );
    }

    /// Task 2.6 / 3.3 — verify admin-overridden `tool_call_prices` reach the
    /// billing engine and fire per actual invocation count from
    /// `usage.tool_uses` (the Phase 3 behavior).
    #[tokio::test]
    async fn admin_tool_call_price_override_affects_billing() {
        let state = build_test_state_for_pricing().await;
        let provider_name = "openai-test";
        let provider_id = state
            .provider_id_for_name(provider_name)
            .expect("provider registered");
        let model_id = "gpt-tool-pricing-test-9998";

        let mut tool_call_prices = std::collections::BTreeMap::new();
        tool_call_prices.insert("web_search".to_string(), 0.05);
        let model_price = gproxy_sdk::provider::billing::ModelPrice {
            model_id: model_id.to_string(),
            display_name: None,
            price_each_call: None,
            price_tiers: Vec::new(),
            flex_price_each_call: None,
            flex_price_tiers: Vec::new(),
            scale_price_each_call: None,
            scale_price_tiers: Vec::new(),
            priority_price_each_call: None,
            priority_price_tiers: Vec::new(),
            tool_call_prices,
        };
        let pricing_json_str =
            crate::bootstrap::model_price_to_storage_json(&model_price).unwrap();

        state
            .storage()
            .upsert_model(gproxy_storage::ModelWrite {
                id: 99998,
                provider_id,
                model_id: model_id.to_string(),
                display_name: None,
                enabled: true,
                price_each_call: None,
                price_tiers_json: None,
                pricing_json: Some(pricing_json_str),
                alias_of: None,
            })
            .await
            .expect("upsert model in storage");
        state.upsert_model_in_memory(MemoryModel {
            id: 99998,
            provider_id,
            model_id: model_id.to_string(),
            display_name: None,
            enabled: true,
            pricing: Some(model_price),
            alias_of: None,
        });
        state.push_pricing_to_engine(provider_name);

        let ctx = BillingContext {
            model_id: model_id.to_string(),
            mode: BillingMode::Default,
        };
        // Phase 3 semantics: charge per actual server_tool_use count. The
        // provider reported 2 web_search invocations; at 0.05 each the
        // expected cost is 0.10.
        let mut usage = Usage::default();
        usage.tool_uses.insert("web_search".to_string(), 2);
        let result = state
            .engine()
            .estimate_billing(provider_name, &ctx, &usage)
            .expect("estimate_billing must return Some");
        assert!(
            (result.total_cost - 0.10).abs() < 1e-9,
            "expected 2 × 0.05 = 0.10, got {}",
            result.total_cost
        );
    }
}
