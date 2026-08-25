mod build;
mod index;
mod pricing;
mod resolve;
mod types;

use std::sync::Arc;

use arc_swap::ArcSwap;
use gproxy_core::{ControlPlane, CoreError, Plan, Pricing, ProviderRef, RoutingMode};
use gproxy_store::records::{
    ControlSnapshot, CredentialQuotaCycleRecord, QuotaBoundarySource, QuotaCycleStatus,
};
use gproxy_store::{Store, StoreError};
use rust_decimal::Decimal;

pub(crate) use types::KeyIdentity;
use types::{CompiledSnapshot, CredentialPressure, CredentialPressureMap};

#[derive(Clone)]
pub(crate) struct SnapshotControl {
    store: Store,
    snapshot: Arc<ArcSwap<CompiledSnapshot>>,
    credential_pressure: Arc<ArcSwap<CredentialPressureMap>>,
}

impl SnapshotControl {
    pub(crate) async fn new(store: Store) -> Result<Self, StoreError> {
        let stored = store.control_snapshot().await?;
        let snapshot = CompiledSnapshot::build(stored)?;
        let credential_pressure = load_pressure(&store).await?;
        Ok(Self {
            store,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
            credential_pressure: Arc::new(ArcSwap::from_pointee(credential_pressure)),
        })
    }

    pub(crate) async fn reload(&self) -> Result<(), StoreError> {
        let stored = self.store.control_snapshot().await?;
        self.snapshot
            .store(Arc::new(CompiledSnapshot::build(stored)?));
        Ok(())
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

    pub(crate) fn key_identity(&self, version: u32, digest: &[u8]) -> Option<KeyIdentity> {
        self.snapshot
            .load()
            .identities
            .get(&(version, digest.to_vec()))
            .cloned()
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
    fn resolve(&self, model: Option<&str>, mode: &RoutingMode) -> Result<Plan, CoreError> {
        let mut plan = self.snapshot.load().resolve(model, mode)?;
        resolve::apply_pressure(&mut plan, &self.credential_pressure.load(), unix_now());
        Ok(plan)
    }

    fn pricing(&self, provider: &ProviderRef, upstream_model: &str) -> Option<Pricing> {
        pricing::resolve(&self.snapshot.load().pricing, provider.id, upstream_model)
    }
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
