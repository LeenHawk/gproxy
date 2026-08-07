//! Live provider model catalogues with additive persistence.
//!
//! Aggregated and scoped model-list requests use the same per-provider policy:
//! refresh live unless disabled in settings or routed `local`. Successful
//! responses add previously unseen ids to `provider_models` and fill missing
//! metadata; skipped, timed-out or failed refreshes use that persisted list.
//! Existing non-null model metadata, variants and enabled state are preserved.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::lock::Mutex;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

use crate::app::AppState;
use crate::app::snapshot::{ControlPlaneSnapshot, KeyIdentity};
use crate::pipeline::authz;
use crate::pipeline::classify;
use crate::pipeline::context::RequestCtx;
use crate::pipeline::error::PipelineError;
use crate::pipeline::local_ops::{self, ModelEntry};
use crate::pipeline::model_limits::{self, ModelLimits, ModelThinking};
use crate::pipeline::outcome::ExecOutcome;
use crate::pipeline::preprocess;
use crate::protocol::{Operation, OperationKey};
use crate::routing::{self, RoutingDecision};
use crate::store::persistence::records::{Provider, ProviderModelInput};

const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
export function gproxyModelListDelay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = gproxyModelListDelay)]
    fn model_list_delay(ms: u32) -> js_sys::Promise;
}

/// Serializes additive provider-model writes and snapshot reloads in this
/// process. Provider catalogues are fetched concurrently, but a final snapshot
/// must never be built from a partially interleaved set of successful writes.
fn persistence_sync_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Add newly discovered upstream ids to `provider_models` and fill metadata
/// gaps on matching rows. Existing non-null limits, enabled state, display names
/// and variants are preserved; models absent from a later response are never deleted.
async fn persist_additions(state: &AppState, provider: &Provider, models: &[ModelEntry]) {
    let _local_guard = persistence_sync_lock().lock().await;
    let lock_key = format!("gproxy:model-catalog:persist-lock:{}", provider.id);
    let lock_owner = crate::util::rand::uuid_v4();
    let acquired = state
        .cache
        .try_lock(&lock_key, &lock_owner, Duration::from_secs(30))
        .await;
    if acquired != crate::store::cache::LockAttempt::Acquired {
        // A peer is syncing this provider and will broadcast invalidation when
        // it commits. This request can still return its live response.
        return;
    }

    let mut changed = false;
    let result: anyhow::Result<()> = async {
        let current = state.persistence.list_provider_models(provider.id).await?;
        let current_arcs: Vec<Arc<crate::store::persistence::records::ProviderModel>> =
            current.iter().cloned().map(Arc::new).collect();
        let compiled = crate::app::models_index::compile(&current_arcs);
        let mut existing: HashSet<String> = current
            .iter()
            .map(|model| model.model_id.clone())
            .chain(compiled.exposed.into_iter().map(|model| model.full_id))
            .collect();

        for model in models {
            let id = model.id.trim();
            if id.is_empty() {
                continue;
            }
            if let Some(saved) = current.iter().find(|saved| saved.model_id == id) {
                let context_window = saved.context_window.or(model.limits.context_window);
                let max_input_tokens = saved.max_input_tokens.or(model.limits.max_input_tokens);
                let max_output_tokens = saved.max_output_tokens.or(model.limits.max_output_tokens);
                let thinking_supported = saved.thinking_supported.or(model.thinking.supported);
                let thinking_adaptive_supported = saved
                    .thinking_adaptive_supported
                    .or(model.thinking.adaptive_supported);
                let thinking_enabled_supported = saved
                    .thinking_enabled_supported
                    .or(model.thinking.enabled_supported);
                if context_window != saved.context_window
                    || max_input_tokens != saved.max_input_tokens
                    || max_output_tokens != saved.max_output_tokens
                    || thinking_supported != saved.thinking_supported
                    || thinking_adaptive_supported != saved.thinking_adaptive_supported
                    || thinking_enabled_supported != saved.thinking_enabled_supported
                {
                    state
                        .persistence
                        .upsert_provider_model(ProviderModelInput {
                            id: Some(saved.id),
                            provider_id: saved.provider_id,
                            model_id: saved.model_id.clone(),
                            display_name: saved.display_name.clone(),
                            variants_json: saved.variants_json.clone(),
                            context_window,
                            max_input_tokens,
                            max_output_tokens,
                            thinking_supported,
                            thinking_adaptive_supported,
                            thinking_enabled_supported,
                            enabled: saved.enabled,
                        })
                        .await?;
                    changed = true;
                }
                continue;
            }
            if !existing.insert(id.to_owned()) {
                continue;
            }
            state
                .persistence
                .upsert_provider_model(ProviderModelInput {
                    id: None,
                    provider_id: provider.id,
                    model_id: id.to_owned(),
                    display_name: model.display_name.clone(),
                    variants_json: None,
                    context_window: model.limits.context_window,
                    max_input_tokens: model.limits.max_input_tokens,
                    max_output_tokens: model.limits.max_output_tokens,
                    thinking_supported: model.thinking.supported,
                    thinking_adaptive_supported: model.thinking.adaptive_supported,
                    thinking_enabled_supported: model.thinking.enabled_supported,
                    enabled: true,
                })
                .await?;
            changed = true;
        }
        Ok(())
    }
    .await;

    if changed {
        crate::app::invalidation::broadcast(state.cache.as_ref(), b"provider-models:auto").await;
        if let Err(error) = state.reload_snapshot().await {
            tracing::warn!(
                provider_id = provider.id,
                error = %error,
                "reload snapshot after automatic model persistence failed"
            );
        }
    }
    state.cache.unlock(&lock_key, &lock_owner).await;
    if let Err(error) = result {
        tracing::warn!(
            provider_id = provider.id,
            error = %error,
            "persist automatic model additions failed"
        );
    }
}

fn manual_entries(cp: &ControlPlaneSnapshot, provider_id: i64) -> Vec<ModelEntry> {
    cp.exposed_models_by_provider
        .get(&provider_id)
        .map(|models| local_ops::entries_from(models))
        .unwrap_or_default()
}

fn merge_catalogue(
    cp: &ControlPlaneSnapshot,
    provider_id: i64,
    remote: Vec<ModelEntry>,
) -> Vec<ModelEntry> {
    let mut seen = HashSet::new();
    manual_entries(cp, provider_id)
        .into_iter()
        .chain(remote)
        .filter(|model| seen.insert(model.id.clone()))
        .collect()
}

fn filter_permitted(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    provider: &Provider,
    models: Vec<ModelEntry>,
) -> Vec<ModelEntry> {
    models
        .into_iter()
        .filter(|model| authz::provider_model_permitted(cp, identity, &provider.name, &model.id))
        .collect()
}

async fn fetch_live(state: &AppState, provider: &Provider) -> Option<Vec<ModelEntry>> {
    let fetch = crate::credentials::upstream_models::fetch_models(state, provider.id);

    #[cfg(not(target_arch = "wasm32"))]
    let result = match tokio::time::timeout(MODEL_LIST_TIMEOUT, fetch).await {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!(
                provider_id = provider.id,
                timeout_ms = MODEL_LIST_TIMEOUT.as_millis(),
                "upstream model catalogue timed out; using persisted models"
            );
            return None;
        }
    };

    #[cfg(target_arch = "wasm32")]
    let result = {
        use futures_util::FutureExt;
        use futures_util::future::{Either, select};

        let fetch = fetch.fuse();
        let delay = JsFuture::from(model_list_delay(MODEL_LIST_TIMEOUT.as_millis() as u32))
            .map(|_| ())
            .fuse();
        futures_util::pin_mut!(fetch, delay);
        match select(fetch, delay).await {
            Either::Left((result, _)) => result,
            Either::Right(_) => {
                tracing::warn!(
                    provider_id = provider.id,
                    timeout_ms = MODEL_LIST_TIMEOUT.as_millis(),
                    "upstream model catalogue timed out; using persisted models"
                );
                return None;
            }
        }
    };

    match result {
        Ok(models) => Some(
            models
                .into_iter()
                .map(|model| ModelEntry {
                    id: model.id,
                    display_name: model.display_name,
                    limits: ModelLimits::new(
                        model.context_window,
                        model.max_input_tokens,
                        model.max_output_tokens,
                    ),
                    thinking: ModelThinking::new(
                        model.thinking_supported,
                        model.thinking_adaptive_supported,
                        model.thinking_enabled_supported,
                    ),
                })
                .collect(),
        ),
        Err(error) => {
            tracing::warn!(
                provider_id = provider.id,
                error = %error,
                "upstream model catalogue failed; using persisted models"
            );
            None
        }
    }
}

fn automatic_refresh_enabled(provider: &Provider) -> bool {
    provider
        .settings_json
        .get("auto_refresh_models")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

fn should_refresh(cp: &ControlPlaneSnapshot, provider: &Provider, source: OperationKey) -> bool {
    let rules = cp
        .routing_rules_by_provider
        .get(&provider.id)
        .map(|rules| rules.as_slice())
        .unwrap_or(&[]);
    automatic_refresh_enabled(provider)
        && !matches!(routing::decide(rules, source), RoutingDecision::Local)
}

/// Serve one scoped model-list request. Eligible live success persists
/// additions; disabled/local refresh and failure/timeout use accumulated rows.
pub async fn serve_scoped(
    state: &AppState,
    provider: Arc<Provider>,
    identity: Arc<KeyIdentity>,
    source: OperationKey,
) -> ExecOutcome {
    let models = models_for_request(state, &provider, source).await;
    let cp = state.cp();
    let entries = filter_permitted(&cp, &identity, &provider, models);
    local_ops::json_outcome(
        http::StatusCode::OK,
        local_ops::render_model_list(source.provider_family(), &entries),
    )
}

/// Shared catalogue lookup used by aggregated and scoped listings.
pub async fn models_for_request(
    state: &AppState,
    provider: &Provider,
    source: OperationKey,
) -> Vec<ModelEntry> {
    {
        let cp = state.cp();
        if !should_refresh(&cp, provider, source) {
            return manual_entries(&cp, provider.id);
        }
    }
    match fetch_live(state, provider).await {
        Some(models) => {
            persist_additions(state, provider, &models).await;
            merge_catalogue(&state.cp(), provider.id, models)
        }
        None => manual_entries(&state.cp(), provider.id),
    }
}

/// Serve aggregated ListModels/GetModel. Eligible provider catalogues refresh
/// concurrently before aliases, routes, and provider models are merged.
pub(crate) async fn serve_aggregated(
    state: &AppState,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    let op = ctx.op.expect("classified");
    let family = op.provider_family();
    let identity = ctx.identity.as_ref().expect("auth ran first");
    let namespace = match &ctx.mode {
        crate::pipeline::context::RoutingMode::Namespace { namespace } => Some(namespace.as_str()),
        _ => None,
    };

    let body = match op.operation() {
        Operation::ListModels => {
            let providers: Vec<Arc<Provider>> = if namespace.is_some() {
                Vec::new()
            } else {
                let cp = state.cp();
                cp.providers_by_name
                    .values()
                    .filter(|provider| {
                        provider.enabled
                            && authz::provider_listing_permitted(&cp, identity, &provider.name)
                    })
                    .cloned()
                    .collect()
            };
            let catalogues = futures_util::future::join_all(
                providers
                    .iter()
                    .map(|provider| models_for_request(state, provider, op)),
            )
            .await;

            let cp = state.cp();
            let route_ids: Vec<String> = match namespace {
                Some(namespace) => cp
                    .routes_by_namespace
                    .get(namespace)
                    .into_iter()
                    .flat_map(|routes| routes.keys())
                    .cloned()
                    .collect(),
                None => cp.routes_by_name.keys().cloned().collect(),
            };
            let mut entries: Vec<ModelEntry> = route_ids
                .iter()
                .filter(|id| authz::permitted(&cp, identity, id))
                .map(|id| ModelEntry {
                    id: id.clone(),
                    display_name: None,
                    limits: model_limits::for_target(&cp, id),
                    thinking: model_limits::thinking_for_target(&cp, id),
                })
                .collect();
            if let Some(global_aliases) = cp.aliases_by_provider.get("*") {
                entries.extend(
                    global_aliases
                        .iter()
                        .filter(|alias| {
                            namespace.map_or_else(
                                || target_permitted(&cp, identity, &alias.target),
                                |namespace| {
                                    namespace_target_permitted(
                                        &cp,
                                        identity,
                                        namespace,
                                        &alias.target,
                                    )
                                },
                            )
                        })
                        .map(|alias| ModelEntry {
                            id: alias.alias.clone(),
                            display_name: None,
                            limits: model_limits::for_target(&cp, &alias.target),
                            thinking: model_limits::thinking_for_target(&cp, &alias.target),
                        }),
                );
            }
            for (provider, provider_models) in providers.iter().zip(catalogues) {
                entries.extend(provider_models.into_iter().filter_map(|mut model| {
                    authz::provider_model_permitted(&cp, identity, &provider.name, &model.id).then(
                        || {
                            model.id = format!("{}/{}", provider.name, model.id);
                            model
                        },
                    )
                }));
                if let Some(aliases) = cp.aliases_by_provider.get(&provider.name) {
                    entries.extend(aliases.iter().filter_map(|alias| {
                        let target =
                            preprocess::apply_provider_alias(&cp, &provider.name, &alias.alias);
                        authz::provider_model_permitted(&cp, identity, &provider.name, &target)
                            .then(|| ModelEntry {
                                id: format!("{}/{}", provider.name, alias.alias),
                                display_name: None,
                                limits: model_limits::for_provider_model(&cp, provider.id, &target),
                                thinking: model_limits::thinking_for_provider_model(
                                    &cp,
                                    provider.id,
                                    &target,
                                ),
                            })
                    }));
                }
            }
            entries.sort_by(|left, right| left.id.cmp(&right.id));
            entries.dedup_by(|left, right| left.id == right.id);
            local_ops::render_model_list(family, &entries)
        }
        _ => {
            let cp = state.cp();
            let id = classify::path_model_id(&ctx.path).ok_or(PipelineError::UnsupportedPath)?;
            let target = preprocess::apply_global_alias(&cp, &id);
            let permitted = namespace.map_or_else(
                || resolved_target_permitted(&cp, identity, &id),
                |namespace| namespace_target_permitted(&cp, identity, namespace, &target),
            );
            if !permitted {
                return Err(PipelineError::UnknownRoute(id));
            }
            local_ops::render_model(
                family,
                &ModelEntry {
                    id,
                    display_name: None,
                    limits: model_limits::for_target(&cp, &target),
                    thinking: model_limits::thinking_for_target(&cp, &target),
                },
            )
        }
    };
    Ok(local_ops::json_outcome(http::StatusCode::OK, body))
}

fn namespace_target_permitted(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    namespace: &str,
    target: &str,
) -> bool {
    cp.routes_by_namespace
        .get(namespace)
        .is_some_and(|routes| routes.contains_key(target))
        && authz::permitted(cp, identity, target)
}

fn target_permitted(cp: &ControlPlaneSnapshot, identity: &KeyIdentity, target: &str) -> bool {
    if cp.routes_by_name.contains_key(target) {
        return authz::permitted(cp, identity, target);
    }
    preprocess::split_provider_model(target)
        .and_then(|(provider_name, model)| {
            let provider = cp
                .providers_by_name
                .get(provider_name)
                .filter(|provider| provider.enabled)?;
            let model = preprocess::apply_provider_alias(cp, provider_name, model);
            Some(authz::provider_model_permitted(
                cp,
                identity,
                &provider.name,
                &model,
            ))
        })
        .unwrap_or(false)
}

fn resolved_target_permitted(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    model: &str,
) -> bool {
    let target = preprocess::apply_global_alias(cp, model);
    target_permitted(cp, identity, &target)
}
