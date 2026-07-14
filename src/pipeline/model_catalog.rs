//! Live provider model catalogues with additive persistence.
//!
//! Aggregated and scoped model-list requests both attempt each relevant
//! provider's live catalogue under a short total timeout. Successful responses
//! add previously unseen ids to `provider_models`; timeout/failure falls back to
//! that persisted list. Existing rows and variants are never changed or removed.

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
use crate::pipeline::local_ops::{self, ModelEntry};
use crate::pipeline::outcome::ExecOutcome;
use crate::protocol::Provider as ProtocolProvider;
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

/// Add newly discovered upstream ids to `provider_models`. This is deliberately
/// monotonic: existing rows (including disabled rows, custom display names and
/// variants) are untouched, and models absent from a later upstream response
/// are never deleted.
async fn persist_additions(state: &AppState, provider: &Provider, models: &[ModelEntry]) {
    let _local_guard = persistence_sync_lock().lock().await;
    let lock_key = format!("gproxy:model-catalog:persist-lock:{}", provider.id);
    let acquired = state
        .cache
        .try_lock(&lock_key, Duration::from_secs(30))
        .await;
    if !acquired {
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
            if id.is_empty() || !existing.insert(id.to_owned()) {
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
    state.cache.unlock(&lock_key).await;
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

fn merge_and_filter(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    provider: &Provider,
    remote: Vec<ModelEntry>,
) -> Vec<ModelEntry> {
    let mut seen = HashSet::new();
    remote
        .into_iter()
        .chain(manual_entries(cp, provider.id))
        .filter(|model| seen.insert(model.id.clone()))
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

/// Serve one scoped model-list request. Live success persists additions;
/// failure/timeout keeps serving the accumulated provider models.
pub async fn serve_scoped(
    state: &AppState,
    provider: Arc<Provider>,
    identity: Arc<KeyIdentity>,
    family: ProtocolProvider,
) -> ExecOutcome {
    let remote = refresh_or_persisted(state, &provider).await;
    let cp = state.cp();
    let entries = merge_and_filter(&cp, &identity, &provider, remote);
    local_ops::json_outcome(
        http::StatusCode::OK,
        local_ops::render_model_list(family, &entries),
    )
}

/// Shared refresh used by both aggregated and scoped listings.
pub async fn refresh_or_persisted(state: &AppState, provider: &Provider) -> Vec<ModelEntry> {
    match fetch_live(state, provider).await {
        Some(models) => {
            persist_additions(state, provider, &models).await;
            models
        }
        None => manual_entries(&state.cp(), provider.id),
    }
}
