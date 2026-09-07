mod balance;
mod build;
mod capability;
mod index;
mod materialize;
mod pressure;
mod pricing;
mod resolve;
mod rules;
mod types;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use gproxy_core::{
    ControlPlane, CoreError, CredentialRecord, Plan, Pricing, ProviderRef, RoutingMode,
};
use gproxy_store::records::{
    ControlSnapshot, CredentialQuotaCycleRecord, QuotaBoundarySource, QuotaCycleStatus,
};
use gproxy_store::{Store, StoreError};
use rust_decimal::Decimal;

use types::CredentialHealthMap;
pub(crate) use types::KeyIdentity;
use types::{CompiledSnapshot, CredentialPressure, CredentialPressureMap};

#[derive(Clone)]
pub(crate) struct SnapshotControl {
    store: Store,
    runtime: super::settings::RuntimeOverrides,
    snapshot: Arc<ArcSwap<CompiledSnapshot>>,
    credential_pressure: Arc<ArcSwap<CredentialPressureMap>>,
    credential_health: Arc<ArcSwap<CredentialHealthMap>>,
    rotation: Arc<balance::RotationCounters>,
    oauth_keys: Arc<ArcSwap<std::collections::BTreeSet<i64>>>,
    /// Decrypted credentials, keyed by id. Every path that changes a stored
    /// credential ends in `reload`, which drops the whole map; a rotation
    /// this instance performs forgets its own entry immediately.
    credential_records: Arc<Mutex<HashMap<i64, CredentialRecord>>>,
    health_persisted_at: Arc<Mutex<HashMap<(gproxy_channel_api::CredentialId, String), i64>>>,
}

impl SnapshotControl {
    pub(crate) fn has_named_target(&self, name: &str) -> bool {
        self.snapshot.load().has_named_target(name)
    }

    pub(crate) async fn new(
        store: Store,
        runtime: super::settings::RuntimeOverrides,
    ) -> Result<Self, StoreError> {
        let stored = store.control_snapshot().await?;
        let snapshot = CompiledSnapshot::build(stored, &runtime)?;
        let credential_pressure = load_pressure(&store).await?;
        let credential_health = load_health(&store).await?;
        let oauth_keys = store.oauth_user_key_ids().await?.into_iter().collect();
        Ok(Self {
            store,
            runtime,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
            credential_pressure: Arc::new(ArcSwap::from_pointee(credential_pressure)),
            credential_health: Arc::new(ArcSwap::from_pointee(credential_health)),
            rotation: Arc::new(balance::RotationCounters::default()),
            oauth_keys: Arc::new(ArcSwap::from_pointee(oauth_keys)),
            credential_records: Arc::default(),
            health_persisted_at: Arc::default(),
        })
    }

    pub(crate) async fn reload(&self) -> Result<(), StoreError> {
        let stored = self.store.control_snapshot().await?;
        let health = load_health(&self.store).await?;
        self.oauth_keys.store(Arc::new(
            self.store.oauth_user_key_ids().await?.into_iter().collect(),
        ));
        self.snapshot
            .store(Arc::new(CompiledSnapshot::build(stored, &self.runtime)?));
        self.credential_health.store(Arc::new(health));
        self.credential_records
            .lock()
            .expect("credential cache")
            .clear();
        Ok(())
    }

    pub(crate) fn cached_credential(&self, id: i64) -> Option<CredentialRecord> {
        self.credential_records
            .lock()
            .expect("credential cache")
            .get(&id)
            .cloned()
    }

    pub(crate) fn cache_credential(&self, record: &CredentialRecord) {
        self.credential_records
            .lock()
            .expect("credential cache")
            .insert(record.id.0, record.clone());
    }

    pub(crate) fn forget_credential(&self, id: i64) {
        self.credential_records
            .lock()
            .expect("credential cache")
            .remove(&id);
    }

    /// Whether an unchanged health observation is worth persisting again:
    /// the row only carries `observed_at`, so refreshing it more than once per
    /// interval buys nothing and costs a commit per request under load.
    pub(crate) fn health_refresh_due(
        &self,
        credential: gproxy_channel_api::CredentialId,
        model: &str,
        now: i64,
    ) -> bool {
        const HEALTH_REFRESH_INTERVAL_SECS: i64 = 30;
        let mut persisted = self.health_persisted_at.lock().expect("health cache");
        let key = (credential, model.to_owned());
        match persisted.get(&key) {
            Some(at) if now - at < HEALTH_REFRESH_INTERVAL_SECS => false,
            _ => {
                persisted.insert(key, now);
                true
            }
        }
    }

    pub(crate) fn credential_health_state(
        &self,
        credential: gproxy_channel_api::CredentialId,
        model: &str,
    ) -> Option<(u64, gproxy_store::records::CredentialHealthState)> {
        self.credential_health
            .load()
            .get(&credential)?
            .get(model)
            .copied()
    }

    pub(crate) fn observe_credential_health(
        &self,
        input: &gproxy_store::records::CredentialHealthInput,
    ) {
        let credential = gproxy_channel_api::CredentialId(input.credential_id);
        let model = input.model.clone();
        let version = input.credential_version;
        let state = input.state;
        self.credential_health.rcu(|current| {
            let mut updated = (**current).clone();
            updated
                .entry(credential)
                .or_default()
                .insert(model.clone(), (version, state));
            Arc::new(updated)
        });
    }

    pub(crate) async fn observe_credential_quota_cycle(
        &self,
        observation: &gproxy_store::records::CredentialQuotaObservation,
    ) -> Result<gproxy_store::records::CredentialQuotaCycleRecord, StoreError> {
        if !self
            .snapshot
            .load()
            .stored
            .credentials
            .iter()
            .any(|credential| credential.id == observation.credential_id)
        {
            return Err(StoreError::InvalidData {
                field: "credential_id",
                message: format!(
                    "credential {} is absent from the control snapshot",
                    observation.credential_id
                ),
            });
        }
        let cycle = self
            .store
            .observe_credential_quota_cycle(observation)
            .await?;
        self.update_pressure(&cycle);
        Ok(cycle)
    }

    pub(crate) async fn close_credential_quota_cycle(
        &self,
        id: i64,
        reason: gproxy_store::records::QuotaCycleCloseReason,
        closed_at: i64,
    ) -> Result<Option<CredentialQuotaCycleRecord>, StoreError> {
        let cycle = self
            .store
            .close_credential_quota_cycle(id, reason, closed_at)
            .await?;
        if let Some(cycle) = cycle.as_ref() {
            self.update_pressure(cycle);
        }
        Ok(cycle)
    }

    pub(crate) fn current(&self) -> Arc<ControlSnapshot> {
        self.snapshot.load().stored.clone()
    }

    pub(crate) fn settings(&self) -> super::settings::EffectiveSettings {
        self.snapshot.load().settings.clone()
    }

    pub(crate) fn provider(&self, id: i64) -> Option<ProviderRef> {
        self.snapshot.load().providers.get(&id).cloned()
    }

    pub(crate) fn key_identity(&self, version: u32, digest: &[u8]) -> Option<KeyIdentity> {
        self.snapshot
            .load()
            .identities
            .get(&(version, digest.to_vec()))
            .cloned()
            .filter(|identity| !self.is_oauth_key(identity.caller.user_key_id))
    }

    pub(crate) fn is_oauth_key(&self, id: i64) -> bool {
        self.oauth_keys.load().contains(&id)
    }

    fn update_pressure(&self, cycle: &CredentialQuotaCycleRecord) {
        let credential = gproxy_channel_api::CredentialId(cycle.credential_id);
        let window_key = cycle.window_key.clone();
        let next = cycle_pressure(cycle);
        self.credential_pressure.rcu(|current| {
            let mut updated = (**current).clone();
            let windows = updated.entry(credential).or_default();
            let replace = windows.get(&window_key).is_none_or(|stored| {
                (stored.last_observed_at, stored.cycle_id, stored.version)
                    <= (cycle.last_observed_at, cycle.id, cycle.version)
            });
            if replace {
                match next.clone() {
                    Some(next) => {
                        windows.insert(window_key.clone(), next);
                    }
                    None => {
                        windows.remove(&window_key);
                    }
                }
            }
            if windows.is_empty() {
                updated.remove(&credential);
            }
            Arc::new(updated)
        });
    }
}

impl ControlPlane for SnapshotControl {
    fn resolve_alias(&self, model: &str, mode: &RoutingMode) -> String {
        self.snapshot.load().resolve_alias(model, mode)
    }

    fn resolve_variant(&self, model: &str, mode: &RoutingMode) -> Option<String> {
        self.snapshot.load().resolve_variant(model, mode)
    }

    fn resolve_preprocessed(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError> {
        let mut plan = self.snapshot.load().resolve_preprocessed(
            model,
            mode,
            affinity,
            &self.credential_health.load(),
            &self.rotation,
        )?;
        pressure::apply(&mut plan, &self.credential_pressure.load(), unix_now());
        Ok(plan)
    }

    fn pricing(&self, provider: &ProviderRef, upstream_model: &str) -> Option<Pricing> {
        pricing::resolve(&self.snapshot.load().pricing, provider.id, upstream_model)
    }

    fn shared(&self) -> Option<Arc<dyn ControlPlane>> {
        Some(Arc::new(self.clone()))
    }

    fn provider_catalogue(&self) -> Vec<gproxy_core::ExposedModel> {
        self.snapshot.load().provider_catalogue.clone()
    }

    fn exposed_models(&self) -> Vec<gproxy_core::ExposedModel> {
        self.snapshot
            .load()
            .model_catalogue
            .values()
            .cloned()
            .collect()
    }

    fn catalogue_visible(
        &self,
        identity: &gproxy_channel_api::CallerIdentity,
        model: Option<&str>,
        mode: &RoutingMode,
    ) -> bool {
        let Ok(plan) = self.resolve(model, mode, Some(identity.user_key_id)) else {
            return false;
        };
        let snapshot = self.current();
        let oauth = self.is_oauth_key(identity.user_key_id);
        plan.targets.iter().any(|target| {
            crate::host::catalogue_permitted(&snapshot, identity, target.provider.id, oauth)
        })
    }

    fn detached(&self) -> Box<dyn ControlPlane> {
        Box::new(self.clone())
    }
}

async fn load_health(store: &Store) -> Result<CredentialHealthMap, StoreError> {
    let mut health = CredentialHealthMap::new();
    for record in store.credential_health().await? {
        health
            .entry(gproxy_channel_api::CredentialId(record.credential_id))
            .or_default()
            .insert(record.model, (record.credential_version, record.state));
    }
    Ok(health)
}

async fn load_pressure(store: &Store) -> Result<CredentialPressureMap, StoreError> {
    let mut by_credential = CredentialPressureMap::new();
    for pressure in store.credential_quota_pressures(unix_now()).await? {
        by_credential
            .entry(gproxy_channel_api::CredentialId(pressure.credential_id))
            .or_default()
            .insert(
                pressure.window_key,
                CredentialPressure {
                    cycle_id: pressure.cycle_id,
                    version: pressure.version,
                    last_observed_at: pressure.last_observed_at,
                    used_percent: pressure.used_percent,
                    period_end: pressure.period_end,
                },
            );
    }
    Ok(by_credential)
}

fn cycle_pressure(cycle: &CredentialQuotaCycleRecord) -> Option<CredentialPressure> {
    if cycle.status != QuotaCycleStatus::Open {
        return None;
    }
    let used_percent = cycle.used_percent.or_else(|| {
        let used = cycle.upstream_used?;
        let limit = cycle.upstream_limit?;
        (limit > Decimal::ZERO).then(|| used / limit * Decimal::ONE_HUNDRED)
    })?;
    Some(CredentialPressure {
        cycle_id: cycle.id,
        version: cycle.version,
        last_observed_at: cycle.last_observed_at,
        used_percent,
        period_end: (cycle.boundary_source == QuotaBoundarySource::Upstream)
            .then_some(cycle.period_end)
            .flatten(),
    })
}

fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
