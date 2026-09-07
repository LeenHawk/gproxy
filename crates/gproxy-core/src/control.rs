//! The control-plane read model and execution plans.
//!
//! Tier 2 resolves a request into a [`Plan`] through the [`ControlPlane`]
//! trait — `gproxy-app` implements it over its ArcSwap snapshot, an
//! embedder over static config. An embedder may also skip resolution
//! entirely and hand [`crate::Core::execute_planned`] a `Plan` it built
//! itself; both entries end in the same engine.

use crate::boundary::RoutingMode;
use crate::error::CoreError;
use crate::host::CredentialId;

mod pricing;
mod service_tier;

pub use pricing::{ConditionalMetricRate, Pricing, PricingTier};
pub use service_tier::{PRICING_SERVICE_TIERS, normalize_service_tier, response_service_tier};

/// Read-only view of routing and pricing state. Synchronous by design:
/// implementations answer from an in-memory snapshot, never from I/O on
/// the hot path (v2's §7.2 model, kept).
pub trait ControlPlane: gproxy_channel_api::MaybeSend + gproxy_channel_api::MaybeSync {
    /// Apply configured aliases before suffix interpretation. Global aliases
    /// run first; implementations may then apply a provider-scoped alias when
    /// `mode` identifies that provider.
    fn resolve_alias(&self, model: &str, mode: &RoutingMode) -> String {
        let _ = mode;
        model.to_owned()
    }

    /// Resolve a requested model under a routing mode into an ordered
    /// candidate plan (route members or a scoped provider's pool).
    fn resolve(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError> {
        let model = model.map(|model| self.resolve_alias(model, mode));
        let model = model.map(|model| self.resolve_variant(&model, mode).unwrap_or(model));
        self.resolve_preprocessed(model.as_deref(), mode, affinity)
    }

    /// Resolve a model variant declared by the catalogue to its base model.
    fn resolve_variant(&self, model: &str, mode: &RoutingMode) -> Option<String> {
        let _ = (model, mode);
        None
    }

    /// Route an already alias- and suffix-resolved model.
    fn resolve_preprocessed(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError>;

    /// Pricing for settlement. `None` settles at zero cost with a warning
    /// rather than refusing the request.
    fn pricing(&self, provider: &ProviderRef, upstream_model: &str) -> Option<Pricing>;
    /// A handle that outlives the call, for upstream work a host detaches from
    /// the response it has already opened. `None` keeps that work inline.
    fn shared(&self) -> Option<std::sync::Arc<dyn ControlPlane>> {
        None
    }

    /// Gateway-visible model ids. Implementations answer from the same
    /// in-memory snapshot used by [`Self::resolve`].
    fn exposed_models(&self) -> Vec<ExposedModel>;

    fn catalogue_visible(
        &self,
        _identity: &gproxy_channel_api::CallerIdentity,
        _model: Option<&str>,
        _mode: &RoutingMode,
    ) -> bool {
        true
    }

    /// What each provider is recorded as serving, namespaced as `provider/model`.
    /// This is the operator's list: a row disabled here never reaches a client,
    /// and a row edited here keeps its limits when discovery runs again.
    fn provider_catalogue(&self) -> Vec<ExposedModel> {
        Vec::new()
    }

    /// Owned view for a long-lived task that outlives the execute call.
    fn detached(&self) -> Box<dyn ControlPlane>;
}

/// One model an upstream reported for a provider, before any operator decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub model_id: String,
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
}

pub use gproxy_channel_api::ModelInfo as ExposedModel;

/// The ordered candidates one request may try, plus the failover budget.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Tried in order until one succeeds or the budget is spent.
    pub targets: Vec<Target>,
    pub budget: FailoverBudget,
}

/// One (provider, credential, model) candidate.
#[derive(Debug, Clone)]
pub struct Target {
    pub provider: ProviderRef,
    pub credential: CredentialId,
    /// The model id the upstream actually receives (after alias/variant
    /// mapping).
    pub upstream_model: String,
    pub tier: u32,
    pub rules: TargetRules,
}

#[derive(Debug, Clone, Default)]
pub struct TargetRules {
    pub routing: std::sync::Arc<[crate::routing::CompiledRoutingRule]>,
    pub process: std::sync::Arc<[crate::process::CompiledRule]>,
}

/// Provider identity plus the channel that talks to it. Settings carry the
/// per-provider knobs a channel reads (base_url overrides, shaping flags).
#[derive(Debug, Clone)]
pub struct ProviderRef {
    pub id: i64,
    pub name: String,
    pub channel: String,
    pub settings: serde_json::Value,
    pub fingerprint: Option<ConfiguredFingerprint>,
    pub proxy_url: Option<String>,
    pub traffic_blacklist: gproxy_channel_api::TrafficBlacklistConfig,
}

#[derive(Debug, Clone)]
pub enum ConfiguredFingerprint {
    Usable(Box<FingerprintOverride>),
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct FingerprintOverride {
    pub headers: http::HeaderMap,
    pub profile: Option<gproxy_channel_api::ClientProfile>,
}

#[derive(Debug, Clone)]
pub struct UpstreamProxy(pub String);

#[derive(Debug, Clone, Copy)]
pub struct FailoverBudget {
    /// Max upstream attempts, counting the first.
    pub max_attempts: u32,
}
