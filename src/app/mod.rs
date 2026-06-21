//! Shared application state handed to every HTTP handler.

pub mod bootstrap;
pub mod export;
pub mod import;
pub mod invalidation;
// MIGRATE-V1 (remove in 2.1): one-shot legacy v1→v2 data migration.
#[cfg(feature = "migrate-v1")]
pub mod migrate_v1;
pub mod models_index;
pub mod retention;
pub mod snapshot;
pub mod update_status;

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::channel::registry::ChannelRegistry;
use crate::config::RuntimeConfig;
use crate::http::client::UpstreamClient;
use crate::store::cache::CacheBackend;
use crate::store::persistence::PersistenceBackend;
use snapshot::ControlPlaneSnapshot;

/// Cheap-to-clone bundle of shared services (everything behind `Arc`).
/// Cloned per request by axum's state extractor.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RuntimeConfig>,
    pub cache: Arc<dyn CacheBackend>,
    pub persistence: Arc<dyn PersistenceBackend>,
    pub upstream: Arc<dyn UpstreamClient>,
    /// Sole control-plane snapshot (§7.2); replaced wholesale on invalidation.
    pub snapshot: Arc<ArcSwap<ControlPlaneSnapshot>>,
    /// Channel adapters keyed by id (§6.3).
    pub channels: Arc<ChannelRegistry>,
    /// Envelope cipher for stored secrets (§14.1): seals at import, opens at
    /// use. Keyless boots get a [`crate::crypto::NoopCipher`] (plaintext).
    pub cipher: Arc<dyn crate::crypto::SecretCipher>,
    /// Per-instance passive health: breakers, credential cooldowns, latency
    /// EWMA (§3.2). Soft state — restart clears.
    pub health: Arc<crate::health::HealthState>,
    /// Single-flight OAuth refresh orchestrator (§14.5). Serialises concurrent
    /// refreshes per credential id so a rotating refresh_token is not burned
    /// twice. Soft state — restart clears.
    pub refresh: Arc<crate::credentials::refresh::RefreshOrchestrator>,
    /// Per-proxy upstream client pool (§7.4): failover resolves the effective
    /// proxy per attempt and picks a client here. `upstream` stays the default
    /// client (also used by tokenizer downloads). wasm/non-wreq builds have no
    /// pool — failover uses `upstream` directly.
    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    pub client_pool: Arc<crate::http::client::ClientPool>,
    /// Global tokenizer registry (§6.3), backed by the shared persistence
    /// backend for downloaded vocabs. `main.rs` only flips download
    /// enablement from instance settings before serving.
    #[cfg(feature = "count-local")]
    pub tokenizers: Arc<crate::tokenize::TokenizerRegistry>,
    /// §19.10 in-process self-update status. `idle` at boot; `apply` walks it
    /// downloading→staged|failed. Soft state — a restart clears it.
    pub update_status: Arc<std::sync::Mutex<crate::app::update_status::UpdateStatus>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<RuntimeConfig>,
        cache: Arc<dyn CacheBackend>,
        persistence: Arc<dyn PersistenceBackend>,
        upstream: Arc<dyn UpstreamClient>,
        snapshot: Arc<ArcSwap<ControlPlaneSnapshot>>,
        channels: Arc<ChannelRegistry>,
        cipher: Arc<dyn crate::crypto::SecretCipher>,
    ) -> Self {
        #[cfg(feature = "count-local")]
        let tokenizers = Arc::new(crate::tokenize::TokenizerRegistry::new(
            Arc::clone(&persistence),
            Arc::clone(&upstream),
        ));
        #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
        let client_pool = Arc::new(crate::http::client::ClientPool::new(Arc::clone(&upstream)));
        Self {
            config,
            cache,
            persistence,
            upstream,
            snapshot,
            channels,
            cipher,
            health: Arc::new(crate::health::HealthState::new()),
            refresh: Arc::new(crate::credentials::refresh::RefreshOrchestrator::new()),
            #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
            client_pool,
            #[cfg(feature = "count-local")]
            tokenizers,
            update_status: Arc::new(std::sync::Mutex::new(Default::default())),
        }
    }

    /// Load the current control-plane snapshot pointer (cheap).
    pub fn cp(&self) -> arc_swap::Guard<Arc<ControlPlaneSnapshot>> {
        self.snapshot.load()
    }

    /// Effective default upstream proxy: the Console-set `instance_settings.proxy`
    /// (snapshot-resident), falling back to the CLI/env `--upstream-proxy-url`.
    /// Per-credential and per-provider proxies still override this — see
    /// [`effective_proxy`](crate::channel::resolve::effective_proxy).
    pub fn upstream_proxy_url(&self) -> Option<String> {
        self.cp()
            .proxy
            .clone()
            .or_else(|| self.config.upstream.proxy_url.clone())
    }

    /// Rebuild the snapshot from persistence and swap it in (next version).
    pub async fn reload_snapshot(&self) -> anyhow::Result<()> {
        let next = self.cp().version.wrapping_add(1);
        let snap = ControlPlaneSnapshot::build(self.persistence.as_ref(), next).await?;
        self.snapshot.store(Arc::new(snap));
        Ok(())
    }
}
