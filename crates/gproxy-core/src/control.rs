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

pub use pricing::{Pricing, PricingTier};
pub use service_tier::{normalize_service_tier, response_service_tier};

/// Read-only view of routing and pricing state. Synchronous by design:
/// implementations answer from an in-memory snapshot, never from I/O on
/// the hot path (v2's §7.2 model, kept).
pub trait ControlPlane {
    /// Resolve a requested model under a routing mode into an ordered
    /// candidate plan (route members or a scoped provider's pool).
    fn resolve(&self, model: Option<&str>, mode: &RoutingMode) -> Result<Plan, CoreError>;

    /// Pricing for settlement. `None` settles at zero cost with a warning
    /// rather than refusing the request.
    fn pricing(&self, provider: &ProviderRef, upstream_model: &str) -> Option<Pricing>;
}

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

#[derive(Debug, Clone, Copy)]
pub struct FailoverBudget {
    /// Max upstream attempts, counting the first.
    pub max_attempts: u32,
}
