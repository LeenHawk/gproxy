use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gproxy_channel_api::{CallerIdentity, CredentialId};
use gproxy_core::{Pricing, ProviderRef};
use gproxy_store::records::ControlSnapshot;

#[derive(Debug, Clone)]
pub(crate) struct KeyIdentity {
    pub caller: CallerIdentity,
    pub expires_at: Option<i64>,
}

pub(super) struct CompiledSnapshot {
    pub stored: Arc<ControlSnapshot>,
    pub providers: BTreeMap<i64, ProviderRef>,
    pub provider_names: BTreeMap<String, i64>,
    pub credentials: BTreeMap<i64, Vec<CredentialId>>,
    pub routes: BTreeMap<i64, CompiledRoute>,
    pub route_names: BTreeMap<String, i64>,
    pub exposed: BTreeMap<String, i64>,
    pub namespaces: BTreeMap<String, BTreeMap<String, i64>>,
    pub global_aliases: BTreeMap<String, String>,
    pub provider_aliases: BTreeMap<i64, BTreeMap<String, String>>,
    pub pricing: Vec<CompiledPriceRule>,
    pub identities: BTreeMap<Vec<u8>, KeyIdentity>,
}

pub(super) struct CompiledRoute {
    pub max_attempts: u32,
    pub targets: Vec<TargetSeed>,
}

#[derive(Clone)]
pub(super) struct TargetSeed {
    pub provider_id: i64,
    pub credential: CredentialId,
    pub upstream_model: String,
}

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
