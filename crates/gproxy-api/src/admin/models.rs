use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_sdk::engine::engine::{ExecuteBody, ExecuteRequest};
use gproxy_server::{AppState, MemoryModel, OperationFamily, ProtocolKind};
use gproxy_storage::repository::ModelRepository;
use gproxy_storage::{ModelWrite, Scope};
use std::collections::{BTreeMap, BTreeSet};
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

fn duplicate_model_key_error(item: &ModelWrite) -> HttpError {
    HttpError::bad_request(format!(
        "model '{}' already exists for provider_id {}",
        item.model_id, item.provider_id
    ))
}

fn normalize_manual_model_write(
    existing_models: &[MemoryModel],
    mut item: ModelWrite,
) -> Result<ModelWrite, HttpError> {
    if let Some(existing_by_key) = existing_models
        .iter()
        .find(|m| m.provider_id == item.provider_id && m.model_id == item.model_id)
    {
        if existing_by_key.id != item.id {
            return Err(duplicate_model_key_error(&item));
        }
        item.id = existing_by_key.id;
    }
    Ok(item)
}

fn normalize_batch_model_writes(
    existing_models: &[MemoryModel],
    items: Vec<ModelWrite>,
) -> Result<Vec<ModelWrite>, HttpError> {
    let mut deduped_by_key = BTreeMap::<(i64, String), ModelWrite>::new();
    for item in items {
        deduped_by_key.insert((item.provider_id, item.model_id.clone()), item);
    }

    let mut used_ids: BTreeSet<i64> = existing_models.iter().map(|m| m.id).collect();
    let mut next_id = used_ids.iter().next_back().copied().unwrap_or(0).max(0) + 1;
    let mut normalized = Vec::with_capacity(deduped_by_key.len());

    for ((provider_id, model_id), mut item) in deduped_by_key {
        if let Some(existing) = existing_models
            .iter()
            .find(|m| m.provider_id == provider_id && m.model_id == model_id)
        {
            item.id = existing.id;
        } else if item.id <= 0 || !used_ids.insert(item.id) {
            while used_ids.contains(&next_id) {
                next_id += 1;
            }
            item.id = next_id;
            used_ids.insert(item.id);
            next_id += 1;
        }
        normalized.push(item);
    }

    Ok(normalized)
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
    let existing_models = state.models();
    let payload = normalize_manual_model_write(&existing_models, payload)?;

    // Validate pricing_json up front so we reject malformed input before
    // writing to the DB.
    let pricing: Option<gproxy_sdk::channel::billing::ModelPrice> = payload
        .pricing_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| HttpError::bad_request(format!("invalid pricing_json: {e}")))?
        .map(|mut mp: gproxy_sdk::channel::billing::ModelPrice| {
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
    let existing_models = state.models();
    let items = normalize_batch_model_writes(&existing_models, items)?;

    // Pre-pass: validate every item's pricing_json before writing any of
    // them. Rejecting a batch halfway would leave the DB in a partial
    // state that's annoying to reason about.
    let parsed: Vec<Option<gproxy_sdk::channel::billing::ModelPrice>> = items
        .iter()
        .map(|item| {
            item.pricing_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|e| {
                    HttpError::bad_request(format!(
                        "invalid pricing_json for model {}: {e}",
                        item.model_id
                    ))
                })
                .map(|parsed_opt| {
                    parsed_opt.map(|mut mp: gproxy_sdk::channel::billing::ModelPrice| {
                        mp.model_id = item.model_id.clone();
                        mp.display_name = item.display_name.clone();
                        mp
                    })
                })
        })
        .collect::<Result<_, _>>()?;

    for (item, pricing) in items.iter().zip(parsed) {
        state.storage().upsert_model(item.clone()).await?;
        state.upsert_model_in_memory(MemoryModel {
            id: item.id,
            provider_id: item.provider_id,
            model_id: item.model_id.clone(),
            display_name: item.display_name.clone(),
            enabled: item.enabled,
            pricing,
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

/// Extract model IDs from an OpenAI-format model list response:
/// `{ "data": [{ "id": "..." }, ...] }`.
///
/// `/admin/models/pull` always issues the upstream call with `ProtocolKind::OpenAi`,
/// so every channel's dispatch table (passthrough, xform, or local) delivers a
/// response in this shape — no per-protocol parsing needed.
fn extract_openai_model_ids(body: &[u8]) -> Vec<String> {
    let Ok(resp) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Vec::new();
    };
    resp.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn pull_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<PullModelsPayload>,
) -> Result<Json<PullModelsResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    // Resolve provider_id -> provider_name (engine.execute takes a name).
    let provider_name = resolve_provider_name(&state, payload.provider_id).await?;

    // Always request with OpenAI protocol. Every channel in the codebase
    // registers (ModelList, OpenAi) in its dispatch table — as Passthrough
    // (openai/anthropic/aistudio/groq/...), Xform (vertex/claudecode/
    // geminicli/antigravity convert to their native protocol), or Local
    // (vertexexpress serves a baked catalogue). So the response is always
    // OpenAI-shaped, and
    // we don't need to infer the protocol from the channel name.
    //
    // Body is `{}` (not empty) because user-defined dispatch overrides — e.g.
    // a custom channel with the frontend's anthropic-like / gemini-like
    // template — route (ModelList, OpenAi) through `transform_request`, which
    // calls `serde_json::from_slice::<RequestBody>(body)`. The OpenAi/Claude/
    // Gemini ModelList `RequestBody` are all empty structs, so they parse from
    // `{}` but fail with EOF on an empty buffer. For Passthrough channels the
    // body is sent as the GET payload and ignored by every upstream.
    //
    // Headers are empty on purpose — the admin request's Authorization /
    // Content-Length / Host would leak to the upstream and break it. The
    // channel's finalize_request adds the provider's own auth headers.
    let result = state
        .engine()
        .execute(ExecuteRequest {
            provider: provider_name.clone(),
            operation: OperationFamily::ModelList,
            protocol: ProtocolKind::OpenAi,
            body: b"{}".to_vec(),
            query: None,
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

    let mut models = extract_openai_model_ids(&body);
    models.sort();
    models.dedup();

    Ok(Json(PullModelsResponse { models }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{normalize_batch_model_writes, normalize_manual_model_write};
    use axum::http::StatusCode;
    use gproxy_sdk::channel::billing::{BillingContext, BillingMode};
    use gproxy_sdk::engine::engine::Usage;
    use gproxy_server::{AppState, AppStateBuilder, GlobalConfig, MemoryModel};
    use gproxy_storage::{
        ModelWrite, SeaOrmStorage,
        repository::{ModelRepository, UserRepository},
    };

    fn memory_model(id: i64, provider_id: i64, model_id: &str) -> MemoryModel {
        MemoryModel {
            id,
            provider_id,
            model_id: model_id.to_string(),
            display_name: None,
            enabled: true,
            pricing: None,
        }
    }

    fn model_write(id: i64, provider_id: i64, model_id: &str) -> ModelWrite {
        ModelWrite {
            id,
            provider_id,
            model_id: model_id.to_string(),
            display_name: None,
            enabled: true,
            pricing_json: None,
        }
    }

    #[test]
    fn batch_model_write_retargets_existing_provider_model_key() {
        let existing = vec![memory_model(10, 7, "gpt-test")];
        let normalized =
            normalize_batch_model_writes(&existing, vec![model_write(99, 7, "gpt-test")])
                .expect("normalize batch");

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].id, 10);
        assert_eq!(normalized[0].provider_id, 7);
        assert_eq!(normalized[0].model_id, "gpt-test");
    }

    #[test]
    fn batch_model_write_assigns_fresh_id_on_generated_id_collision() {
        let existing = vec![memory_model(10, 7, "old-model")];
        let normalized =
            normalize_batch_model_writes(&existing, vec![model_write(10, 7, "new-model")])
                .expect("normalize batch");

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].id, 11);
        assert_eq!(normalized[0].model_id, "new-model");
    }

    #[test]
    fn batch_model_write_retargets_existing_key_even_when_generated_id_collides() {
        let existing = vec![
            memory_model(10, 7, "old-model"),
            memory_model(20, 7, "gpt-test"),
        ];
        let normalized =
            normalize_batch_model_writes(&existing, vec![model_write(10, 7, "gpt-test")])
                .expect("normalize batch");

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].id, 20);
        assert_eq!(normalized[0].model_id, "gpt-test");
    }

    #[test]
    fn batch_model_write_dedupes_duplicate_provider_model_keys() {
        let mut first = model_write(1, 7, "gpt-test");
        first.display_name = Some("first".to_string());
        let mut second = model_write(2, 7, "gpt-test");
        second.display_name = Some("second".to_string());

        let normalized =
            normalize_batch_model_writes(&[], vec![first, second]).expect("normalize batch");

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].id, 2);
        assert_eq!(normalized[0].display_name.as_deref(), Some("second"));
    }

    #[test]
    fn batch_model_write_assigns_fresh_ids_for_duplicate_ids_in_batch() {
        let normalized = normalize_batch_model_writes(
            &[],
            vec![
                model_write(10, 7, "first-model"),
                model_write(10, 7, "second-model"),
            ],
        )
        .expect("normalize batch");

        assert_eq!(normalized.len(), 2);
        assert_ne!(normalized[0].id, normalized[1].id);
        assert_eq!(normalized[0].id, 10);
        assert_eq!(normalized[1].id, 1);
    }

    #[test]
    fn manual_model_write_rejects_duplicate_provider_model_key() {
        let existing = vec![memory_model(10, 7, "gpt-test")];
        let err = normalize_manual_model_write(&existing, model_write(99, 7, "gpt-test"))
            .expect_err("manual duplicate must fail");

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("already exists"));
    }

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
                .engine(gproxy_sdk::engine::engine::GproxyEngine::builder().build())
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
        let model_price = gproxy_sdk::channel::billing::ModelPrice {
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
                pricing_json: Some(pricing_json_str),
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
}
