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
use crate::http::client::{ClientError, UpstreamClient};
use crate::store::cache::CacheBackend;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{Credential, Provider};
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

    /// Resolve a client for the current instance-level default proxy. This includes
    /// the hot-reloaded Console setting, not just the startup CLI/env proxy baked
    /// into `self.upstream`.
    pub fn upstream_client_for_default_proxy(
        &self,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        let proxy = self.upstream_proxy_url();
        self.upstream_client_for_proxy(proxy.as_deref())
    }

    /// Resolve a client for provider-scoped auxiliary calls that have no concrete
    /// credential yet (login bootstrap, self-contained provider helpers):
    /// provider proxy first, then the instance/global default.
    pub fn upstream_client_for_provider(
        &self,
        provider: &Provider,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        let proxy = provider
            .proxy_url
            .clone()
            .or_else(|| self.upstream_proxy_url());
        self.upstream_client_for_proxy(proxy.as_deref())
    }

    /// Resolve a client for a provider id if the caller has one, otherwise for the
    /// instance/global default proxy. Used by admin login bootstrap flows where
    /// older clients may not yet send `provider_id` at the first step.
    pub fn upstream_client_for_provider_id(
        &self,
        provider_id: Option<i64>,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        match provider_id {
            Some(id) => {
                let provider = self
                    .cp()
                    .providers_by_id
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| ClientError::Config("provider not found".into()))?;
                self.upstream_client_for_provider(&provider)
            }
            None => self.upstream_client_for_default_proxy(),
        }
    }

    /// Resolve the full traffic client for a concrete credential: effective proxy
    /// plus DB TLS fingerprint or the channel's built-in emulation profile.
    pub fn upstream_client_for_credential(
        &self,
        channel: &Arc<dyn crate::channel::Channel>,
        credential: &Credential,
        provider: &Provider,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        self.upstream_client_for_credential_inner(channel, credential, provider)
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn upstream_client_for_proxy(
        &self,
        proxy: Option<&str>,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        self.client_pool.for_target(proxy, None)
    }

    #[cfg(not(all(not(target_arch = "wasm32"), feature = "upstream-wreq")))]
    fn upstream_client_for_proxy(
        &self,
        _proxy: Option<&str>,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        Ok(Arc::clone(&self.upstream))
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn upstream_client_for_credential_inner(
        &self,
        channel: &Arc<dyn crate::channel::Channel>,
        credential: &Credential,
        provider: &Provider,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        let global_proxy = self.upstream_proxy_url();
        let proxy =
            crate::channel::resolve::effective_proxy(credential, provider, global_proxy.as_deref());
        let fingerprint = crate::channel::resolve::effective_tls_fingerprint(credential, provider);
        if let Some(fp) = fingerprint.as_ref() {
            self.client_pool.for_target(proxy.as_deref(), Some(fp))
        } else if let Some(emu) = channel.default_emulation() {
            self.client_pool
                .for_channel(proxy.as_deref(), channel.id(), emu)
        } else {
            self.client_pool.for_target(proxy.as_deref(), None)
        }
    }

    #[cfg(not(all(not(target_arch = "wasm32"), feature = "upstream-wreq")))]
    fn upstream_client_for_credential_inner(
        &self,
        _channel: &Arc<dyn crate::channel::Channel>,
        _credential: &Credential,
        _provider: &Provider,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        Ok(Arc::clone(&self.upstream))
    }

    /// Rebuild the snapshot from persistence and swap it in (next version).
    pub async fn reload_snapshot(&self) -> anyhow::Result<()> {
        let next = self.cp().version.wrapping_add(1);
        let snap = ControlPlaneSnapshot::build(self.persistence.as_ref(), next).await?;
        self.snapshot.store(Arc::new(snap));
        Ok(())
    }
}
