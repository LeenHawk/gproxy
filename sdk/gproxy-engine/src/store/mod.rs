use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use tokio::sync::broadcast;

use gproxy_channel::channel::{Channel, OAuthFlow};
use gproxy_channel::response::UpstreamError;
use gproxy_channel::routing::RoutingTable;

pub mod public_traits;
mod runtime;
pub mod types;

pub use public_traits::{EngineEventSource, ProviderMutator, ProviderRegistry};
pub use types::{
    CredentialHealthSnapshot, CredentialSnapshot, CredentialUpdate, EngineEvent, OAuthFinishResult,
    ProviderSnapshot,
};

use runtime::ProviderInstance;
pub(crate) use runtime::ProviderRuntime;

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Snapshot of credentials, revision, paired health states, and max retries for a retry cycle.
pub(crate) type RetryState<Cred, Health> = (Arc<Vec<Cred>>, u64, Vec<(Cred, Health)>, u32);

#[derive(Default)]
pub struct ProviderStoreBuilder {
    providers: HashMap<String, Arc<dyn ProviderRuntime>>,
}

impl ProviderStoreBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_provider<C: Channel>(
        self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
    ) -> Self {
        self.add_provider_with_routing(name, channel, settings, credentials, None)
    }

    pub fn add_provider_with_routing<C: Channel>(
        mut self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
        routing_override: Option<RoutingTable>,
    ) -> Self {
        let name = name.into();
        let provider = Arc::new(ProviderInstance::new(
            name.clone(),
            channel,
            settings,
            credentials,
            routing_override,
        ));
        self.providers.insert(name, provider);
        self
    }

    pub fn build(self) -> ProviderStore {
        let providers = DashMap::with_capacity(self.providers.len());
        for (name, provider) in self.providers {
            providers.insert(name, provider);
        }
        let (event_tx, _) = broadcast::channel(64);
        ProviderStore {
            providers,
            event_tx,
        }
    }
}

pub struct ProviderStore {
    providers: DashMap<String, Arc<dyn ProviderRuntime>>,
    event_tx: broadcast::Sender<EngineEvent>,
}

impl ProviderStore {
    pub fn builder() -> ProviderStoreBuilder {
        ProviderStoreBuilder::new()
    }

    pub fn add_provider<C: Channel>(
        &self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
    ) {
        self.add_provider_with_routing(name, channel, settings, credentials, None);
    }

    pub fn add_provider_with_routing<C: Channel>(
        &self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
        routing_override: Option<RoutingTable>,
    ) {
        let name = name.into();
        let provider = Arc::new(ProviderInstance::new(
            name.clone(),
            channel,
            settings,
            credentials,
            routing_override,
        ));
        self.providers.insert(name.clone(), provider);
        self.emit_event(EngineEvent::ProviderAdded { name });
    }

    /// Add or replace a provider from serialized JSON config.
    pub fn add_provider_json(
        &self,
        config: crate::engine::ProviderConfig,
    ) -> Result<(), UpstreamError> {
        macro_rules! add {
            ($self:expr, $ch:expr, $cfg:expr) => {{
                let crate::engine::ProviderConfig {
                    name,
                    settings_json,
                    credentials,
                    routing,
                    ..
                } = $cfg;
                let routing = match routing {
                    Some(document) => Some(
                        gproxy_channel::routing::RoutingTable::from_document(document).map_err(
                            |e| {
                                UpstreamError::Channel(format!(
                                    "invalid routing for '{}': {e}",
                                    name
                                ))
                            },
                        )?,
                    ),
                    None => None,
                };
                let settings = serde_json::from_value(settings_json).map_err(|e| {
                    UpstreamError::Channel(format!("invalid settings for '{}': {e}", name))
                })?;
                let creds: Vec<_> = credentials
                    .into_iter()
                    .filter_map(|c| {
                        serde_json::from_value(c)
                            .ok()
                            .map(|c| (c, gproxy_channel::health::ModelCooldownHealth::default()))
                    })
                    .collect();
                $self.add_provider_with_routing(&name, $ch, settings, creds, routing);
                Ok(())
            }};
        }

        use gproxy_channel::channels::*;

        match config.channel.as_str() {
            #[cfg(feature = "openai")]
            "openai" => add!(self, openai::OpenAiChannel, config),
            #[cfg(feature = "anthropic")]
            "anthropic" => add!(self, anthropic::AnthropicChannel, config),
            #[cfg(feature = "claudecode")]
            "claudecode" => add!(self, claudecode::ClaudeCodeChannel, config),
            #[cfg(feature = "codex")]
            "codex" => add!(self, codex::CodexChannel, config),
            #[cfg(feature = "vertex")]
            "vertex" => add!(self, vertex::VertexChannel, config),
            #[cfg(feature = "vertexexpress")]
            "vertexexpress" => add!(self, vertexexpress::VertexExpressChannel, config),
            #[cfg(feature = "aistudio")]
            "aistudio" => add!(self, aistudio::AiStudioChannel, config),
            #[cfg(feature = "geminicli")]
            "geminicli" => add!(self, geminicli::GeminiCliChannel, config),
            #[cfg(feature = "antigravity")]
            "antigravity" => add!(self, antigravity::AntigravityChannel, config),
            #[cfg(feature = "nvidia")]
            "nvidia" => add!(self, nvidia::NvidiaChannel, config),
            #[cfg(feature = "deepseek")]
            "deepseek" => add!(self, deepseek::DeepSeekChannel, config),
            #[cfg(feature = "groq")]
            "groq" => add!(self, groq::GroqChannel, config),
            #[cfg(feature = "openrouter")]
            "openrouter" => add!(self, openrouter::OpenRouterChannel, config),
            #[cfg(feature = "custom")]
            "custom" => add!(self, custom::CustomChannel, config),
            _ => Err(UpstreamError::Channel(format!(
                "unknown channel: {}",
                config.channel
            ))),
        }
    }

    pub fn remove_provider(&self, name: &str) -> bool {
        let removed = self.providers.remove(name).is_some();
        if removed {
            self.emit_event(EngineEvent::ProviderRemoved {
                name: name.to_string(),
            });
        }
        removed
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderSnapshot>, UpstreamError> {
        self.providers
            .iter()
            .map(|entry| entry.value().snapshot())
            .collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.event_tx.subscribe()
    }

    /// Get health status for all credentials across all providers.
    pub fn list_health(&self, provider_name: Option<&str>) -> Vec<CredentialHealthSnapshot> {
        let mut out = Vec::new();
        for entry in self.providers.iter() {
            if provider_name.is_some_and(|filter| filter != entry.key().as_str()) {
                continue;
            }
            out.extend(entry.value().health_snapshots());
        }
        out
    }

    /// Manually mark a credential as dead.
    pub fn mark_credential_dead(&self, provider_name: &str, index: usize) -> bool {
        if let Some(provider) = self.get_runtime(provider_name) {
            provider.mark_credential_dead(index);
            self.emit_health_change(provider_name, index, &provider);
            true
        } else {
            false
        }
    }

    /// Manually reset a credential to healthy.
    pub fn mark_credential_healthy(&self, provider_name: &str, index: usize) -> bool {
        if let Some(provider) = self.get_runtime(provider_name) {
            provider.mark_credential_healthy(index);
            self.emit_health_change(provider_name, index, &provider);
            true
        } else {
            false
        }
    }

    pub fn get_provider(&self, name: &str) -> Result<Option<ProviderSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(name) else {
            return Ok(None);
        };
        provider.snapshot().map(Some)
    }

    pub fn list_credentials(
        &self,
        provider_name: Option<&str>,
    ) -> Result<Vec<CredentialSnapshot>, UpstreamError> {
        let mut out = Vec::new();
        for entry in self.providers.iter() {
            if provider_name.is_some_and(|filter| filter != entry.key().as_str()) {
                continue;
            }
            out.extend(entry.value().credential_snapshots()?);
        }
        Ok(out)
    }

    pub fn get_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        provider.credential_snapshot(index)
    }

    pub fn update_provider_settings(
        &self,
        provider_name: &str,
        settings: Value,
    ) -> Result<bool, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(false);
        };
        provider.set_settings_json(settings)?;
        self.emit_provider_updated(provider_name);
        Ok(true)
    }

    pub fn add_credential(
        &self,
        provider_name: &str,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let result = provider.add_credential_json(credential).map(Some);
        if result.as_ref().is_ok_and(Option::is_some) {
            self.emit_provider_updated(provider_name);
        }
        tracing::info!(provider = provider_name, "credential added");
        result
    }

    pub fn update_credential(
        &self,
        provider_name: &str,
        index: usize,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let result = provider.update_credential_json(index, credential);
        if result.as_ref().is_ok_and(Option::is_some) {
            self.emit_provider_updated(provider_name);
        }
        tracing::info!(provider = provider_name, index, "credential updated");
        result
    }

    pub fn remove_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let result = provider.remove_credential_json(index);
        if result.as_ref().is_ok_and(Option::is_some) {
            self.emit_provider_updated(provider_name);
        }
        tracing::info!(provider = provider_name, index, "credential removed");
        result
    }

    pub fn apply_credential_update(
        &self,
        update: &CredentialUpdate,
    ) -> Result<bool, UpstreamError> {
        let Some(provider) = self.get_runtime(&update.provider) else {
            return Ok(false);
        };
        let applied = provider.apply_credential_update(update)?;
        if applied {
            self.emit_provider_updated(&update.provider);
        }
        Ok(applied)
    }

    pub fn apply_credential_updates(
        &self,
        updates: &[CredentialUpdate],
    ) -> Result<Vec<bool>, UpstreamError> {
        let mut grouped: HashMap<(String, u64), Vec<(usize, CredentialUpdate)>> = HashMap::new();
        for (index, update) in updates.iter().cloned().enumerate() {
            grouped
                .entry((update.provider.clone(), update.revision))
                .or_default()
                .push((index, update));
        }

        let mut results = vec![false; updates.len()];
        for ((provider_name, _revision), entries) in grouped {
            let Some(provider) = self.get_runtime(&provider_name) else {
                continue;
            };
            let batch: Vec<CredentialUpdate> =
                entries.iter().map(|(_, update)| update.clone()).collect();
            let batch_results = provider.apply_credential_updates(&batch)?;
            let mut provider_updated = false;
            for ((original_index, _), applied) in entries.into_iter().zip(batch_results.into_iter())
            {
                results[original_index] = applied;
                provider_updated |= applied;
            }
            if provider_updated {
                self.emit_provider_updated(&provider_name);
            }
        }
        Ok(results)
    }

    pub async fn oauth_start(
        &self,
        provider_name: &str,
        client: &wreq::Client,
        params: HashMap<String, String>,
    ) -> Result<Option<OAuthFlow>, UpstreamError> {
        tracing::info!(provider = provider_name, "oauth flow started");
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        provider.oauth_start(client, &params).await
    }

    pub async fn oauth_finish(
        &self,
        provider_name: &str,
        client: &wreq::Client,
        params: HashMap<String, String>,
    ) -> Result<Option<OAuthFinishResult>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let Some((credential_json, details)) = provider.oauth_finish(client, &params).await? else {
            return Ok(None);
        };
        let Some(credential) = self.add_credential(provider_name, credential_json)? else {
            return Ok(None);
        };
        tracing::info!(provider = provider_name, "oauth flow completed");
        Ok(Some(OAuthFinishResult {
            credential,
            details,
        }))
    }

    /// Get the routing table for a named provider.
    pub fn get_routing_table(&self, name: &str) -> Option<RoutingTable> {
        self.providers
            .get(name)
            .map(|entry| entry.value().routing_table().clone())
    }

    pub fn estimate_billing(
        &self,
        provider_name: &str,
        context: &gproxy_channel::billing::BillingContext,
        usage: &crate::engine::Usage,
    ) -> Option<gproxy_channel::billing::BillingResult> {
        self.get_runtime(provider_name)
            .and_then(|provider| provider.estimate_billing(context, usage))
    }

    /// Replace model pricing for a single provider. Returns `false` if the
    /// provider is not registered.
    pub fn set_model_pricing(
        &self,
        provider_name: &str,
        prices: Vec<gproxy_channel::billing::ModelPrice>,
    ) -> bool {
        match self.get_runtime(provider_name) {
            Some(runtime) => {
                runtime.set_model_pricing(prices);
                true
            }
            None => false,
        }
    }

    /// Build a [`BillingContext`] for a provider using the model name and
    /// raw request body.  Delegates to the provider's channel-specific
    /// billing-mode detection without requiring an engine-internal
    /// [`PreparedRequest`].
    pub fn build_billing_context(
        &self,
        provider_name: &str,
        model: Option<&str>,
        body: &[u8],
    ) -> Option<gproxy_channel::billing::BillingContext> {
        let provider = self.get_runtime(provider_name)?;
        gproxy_channel::billing::build_billing_context_from_parts(
            provider.channel_id(),
            model,
            body,
        )
    }

    pub(crate) fn get_runtime(&self, name: &str) -> Option<Arc<dyn ProviderRuntime>> {
        self.providers
            .get(name)
            .map(|entry| Arc::clone(entry.value()))
    }

    fn emit_event(&self, event: EngineEvent) {
        let _ = self.event_tx.send(event);
    }

    fn emit_provider_updated(&self, name: &str) {
        self.emit_event(EngineEvent::ProviderUpdated {
            name: name.to_string(),
        });
    }

    fn emit_health_change(
        &self,
        provider_name: &str,
        index: usize,
        provider: &Arc<dyn ProviderRuntime>,
    ) {
        let Some(snapshot) = provider
            .health_snapshots()
            .into_iter()
            .find(|snapshot| snapshot.index == index)
        else {
            return;
        };

        self.emit_event(EngineEvent::CredentialHealthChanged {
            provider: provider_name.to_string(),
            index,
            status: snapshot.status,
        });
    }
}

impl ProviderRegistry for ProviderStore {
    fn get_provider(&self, name: &str) -> Result<Option<ProviderSnapshot>, UpstreamError> {
        ProviderStore::get_provider(self, name)
    }

    fn list_providers(&self) -> Result<Vec<ProviderSnapshot>, UpstreamError> {
        ProviderStore::list_providers(self)
    }

    fn get_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        ProviderStore::get_credential(self, provider_name, index)
    }

    fn list_credentials(
        &self,
        provider_name: Option<&str>,
    ) -> Result<Vec<CredentialSnapshot>, UpstreamError> {
        ProviderStore::list_credentials(self, provider_name)
    }
}

impl ProviderMutator for ProviderStore {
    fn upsert_provider_json(
        &self,
        config: crate::engine::ProviderConfig,
    ) -> Result<(), UpstreamError> {
        ProviderStore::add_provider_json(self, config)
    }

    fn remove_provider(&self, name: &str) -> bool {
        ProviderStore::remove_provider(self, name)
    }

    fn upsert_credential_json(
        &self,
        provider_name: &str,
        index: Option<usize>,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        match index {
            Some(index) => ProviderStore::update_credential(self, provider_name, index, credential),
            None => ProviderStore::add_credential(self, provider_name, credential),
        }
    }

    fn remove_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        ProviderStore::remove_credential(self, provider_name, index)
    }
}

impl EngineEventSource for ProviderStore {
    fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        ProviderStore::subscribe(self)
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderStore;
    use gproxy_channel::channels::codex::{CodexChannel, CodexSettings};
    use gproxy_channel::health::ModelCooldownHealth;

    #[test]
    fn set_model_pricing_updates_billing() {
        // Build a store with the Codex channel (any channel works; pricing will be overridden).
        let store = ProviderStore::builder()
            .add_provider(
                "demo",
                CodexChannel,
                CodexSettings::default(),
                vec![(
                    gproxy_channel::channels::codex::CodexCredential {
                        access_token: "token-1".to_string(),
                        ..Default::default()
                    },
                    ModelCooldownHealth::default(),
                )],
            )
            .build();

        let ctx = gproxy_channel::billing::BillingContext {
            model_id: "stub-model".into(),
            mode: gproxy_channel::billing::BillingMode::Default,
        };
        let usage = crate::engine::Usage::default();

        // Seed the initial price to $1.00 via set_model_pricing itself.
        let initial_prices = vec![gproxy_channel::billing::ModelPrice {
            model_id: "stub-model".into(),
            display_name: None,
            price_each_call: Some(1.00),
            price_tiers: Vec::new(),
            flex_price_each_call: None,
            flex_price_tiers: Vec::new(),
            scale_price_each_call: None,
            scale_price_tiers: Vec::new(),
            priority_price_each_call: None,
            priority_price_tiers: Vec::new(),
        }];
        assert!(store.set_model_pricing("demo", initial_prices));

        // Sanity: billing reflects the built-in price.
        let before = store
            .estimate_billing("demo", &ctx, &usage)
            .unwrap()
            .total_cost;
        assert!((before - 1.00).abs() < 1e-9);

        // Override: admin sets price to $2.50.
        let new_prices = vec![gproxy_channel::billing::ModelPrice {
            model_id: "stub-model".into(),
            display_name: None,
            price_each_call: Some(2.50),
            price_tiers: Vec::new(),
            flex_price_each_call: None,
            flex_price_tiers: Vec::new(),
            scale_price_each_call: None,
            scale_price_tiers: Vec::new(),
            priority_price_each_call: None,
            priority_price_tiers: Vec::new(),
        }];
        assert!(store.set_model_pricing("demo", new_prices));

        let after = store
            .estimate_billing("demo", &ctx, &usage)
            .unwrap()
            .total_cost;
        assert!((after - 2.50).abs() < 1e-9);

        // Unknown provider returns false — no panic, no silent success.
        assert!(!store.set_model_pricing("nonexistent", vec![]));
    }

    #[test]
    fn prepare_quota_request_uses_selected_credential_index() {
        let store = ProviderStore::builder()
            .add_provider(
                "demo",
                CodexChannel,
                CodexSettings::default(),
                vec![
                    (
                        gproxy_channel::channels::codex::CodexCredential {
                            access_token: "token-1".to_string(),
                            ..Default::default()
                        },
                        ModelCooldownHealth::default(),
                    ),
                    (
                        gproxy_channel::channels::codex::CodexCredential {
                            access_token: "token-2".to_string(),
                            ..Default::default()
                        },
                        ModelCooldownHealth::default(),
                    ),
                ],
            )
            .build();

        let runtime = store.get_runtime("demo").expect("runtime");
        let request = runtime
            .prepare_quota_request(Some(1))
            .expect("quota request")
            .expect("http request");

        let auth = request
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .expect("authorization header");
        assert_eq!(auth, "Bearer token-2");
    }
}
