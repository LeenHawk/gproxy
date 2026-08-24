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

use types::CompiledSnapshot;
pub(crate) use types::KeyIdentity;

#[derive(Clone)]
pub(crate) struct SnapshotControl {
    store: Store,
    snapshot: Arc<ArcSwap<CompiledSnapshot>>,
}

impl SnapshotControl {
    pub(crate) async fn new(store: Store) -> Result<Self, StoreError> {
        let stored = store.control_snapshot().await?;
        let snapshot = CompiledSnapshot::build(stored)?;
        Ok(Self {
            store,
            snapshot: Arc::new(ArcSwap::from_pointee(snapshot)),
        })
    }

    pub(crate) async fn reload(&self) -> Result<(), StoreError> {
        let stored = self.store.control_snapshot().await?;
        self.snapshot
            .store(Arc::new(CompiledSnapshot::build(stored)?));
        Ok(())
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
        self.snapshot.load().resolve(model, mode)
    }

    fn pricing(&self, provider: &ProviderRef, upstream_model: &str) -> Option<Pricing> {
        pricing::resolve(&self.snapshot.load().pricing, provider.id, upstream_model)
    }
}
