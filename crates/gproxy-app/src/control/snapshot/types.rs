use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gproxy_channel_api::{CallerIdentity, CredentialId};
use gproxy_core::{ConfiguredFingerprint, Pricing, ProviderRef};
use gproxy_store::records::ControlSnapshot;

#[derive(Debug, Clone)]
pub(crate) struct KeyIdentity {
    pub caller: CallerIdentity,
    pub expires_at: Option<i64>,
}

#[derive(Clone)]
pub(super) struct CredentialPressure {
    pub cycle_id: i64,
    pub version: u64,
    pub last_observed_at: i64,
    pub used_percent: rust_decimal::Decimal,
    pub period_end: Option<i64>,
}

pub(super) type CredentialPressureMap =
    BTreeMap<CredentialId, BTreeMap<String, CredentialPressure>>;

pub(super) struct CompiledSnapshot {
    pub stored: Arc<ControlSnapshot>,
    pub providers: BTreeMap<i64, ProviderRef>,
    pub provider_names: BTreeMap<String, i64>,
    pub credentials: BTreeMap<i64, Vec<CredentialSeed>>,
    pub routes: BTreeMap<i64, CompiledRoute>,
    pub route_names: BTreeMap<String, i64>,
    pub exposed: BTreeMap<String, i64>,
    pub namespaces: BTreeMap<String, BTreeMap<String, i64>>,
    pub global_aliases: BTreeMap<String, String>,
    pub provider_aliases: BTreeMap<i64, BTreeMap<String, String>>,
    pub pricing: Vec<CompiledPriceRule>,
    pub identities: BTreeMap<(u32, Vec<u8>), KeyIdentity>,
    pub routing_rules: BTreeMap<i64, Arc<[gproxy_core::routing::CompiledRoutingRule]>>,
    pub process_rules: BTreeMap<i64, Arc<[gproxy_core::process::CompiledRule]>>,
}

pub(super) struct CompiledRoute {
    pub max_attempts: u32,
    pub targets: Vec<TargetSeed>,
}

#[derive(Clone)]
pub(super) struct TargetSeed {
    pub member_id: i64,
    pub tier: u32,
    pub member_weight: u32,
    pub provider_id: i64,
    pub credential: CredentialId,
    pub credential_version: u64,
    pub credential_weight: u32,
    pub proxy_url: Option<String>,
    pub fingerprint: Option<ConfiguredFingerprint>,
    pub upstream_model: String,
}

#[derive(Clone)]
pub(super) struct CredentialSeed {
    pub id: CredentialId,
    pub version: u64,
    pub weight: u32,
    pub proxy_url: Option<String>,
    pub fingerprint: Option<ConfiguredFingerprint>,
}

pub(super) type CredentialHealthMap = BTreeMap<CredentialId, (u64, bool)>;

pub(super) struct CompiledPriceRule {
    pub id: i64,
    pub provider_id: Option<i64>,
    pub model_pattern: String,
    pub priority: i64,
    pub rates: Pricing,
}

pub(super) fn namespace_route_ids(
    routes: &BTreeMap<String, i64>,
) -> impl Iterator<Item = i64> + '_ {
    let mut seen = BTreeSet::new();
    routes.values().copied().filter(move |id| seen.insert(*id))
}
