/// Result of a SQLite size-pressure cleanup. Usage rows and rollups are never
/// included in `removed_rows`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoragePruneResult {
    pub before_bytes: u64,
    pub after_bytes: u64,
    pub removed_rows: u64,
    pub exhausted: bool,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait MaintenancePersistence {
    /// For native SQLite, delete oldest request/audit logs and compact the
    /// database when it exceeds `max_bytes`. Other backends return `None`.
    async fn prune_observability_storage(
        &self,
        max_bytes: u64,
        target_bytes: u64,
    ) -> anyhow::Result<Option<StoragePruneResult>>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl MaintenancePersistence for dyn super::PersistenceBackend + '_ {
    async fn prune_observability_storage(
        &self,
        max_bytes: u64,
        target_bytes: u64,
    ) -> anyhow::Result<Option<StoragePruneResult>> {
        super::PersistenceBackend::prune_observability_storage(self, max_bytes, target_bytes).await
    }
}
