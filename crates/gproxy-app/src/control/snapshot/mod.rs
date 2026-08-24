mod build;
mod index;
mod pricing;
mod resolve;
mod types;

use std::sync::Arc;

use arc_swap::ArcSwap;
use gproxy_core::{ControlPlane, CoreError, Plan, Pricing, ProviderRef, RoutingMode};
use gproxy_store::records::ControlSnapshot;
use gproxy_store::{Store, StoreError};

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
        self.credential_pressure
            .store(Arc::new(load_pressure(&self.store).await?));
        Ok(cycle)
    }

    pub(crate) fn current(&self) -> Arc<ControlSnapshot> {
        self.snapshot.load().stored.clone()
    }

    pub(crate) fn key_identity(&self, digest: &[u8]) -> Option<KeyIdentity> {
        self.snapshot.load().identities.get(digest).cloned()
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
            .push(CredentialPressure {
                used_percent: pressure.used_percent,
                period_end: pressure.period_end,
            });
    }
    Ok(by_credential)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
