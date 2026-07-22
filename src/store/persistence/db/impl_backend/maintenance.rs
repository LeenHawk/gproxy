use async_trait::async_trait;

use super::super::{DbPersistence, ops};
use crate::store::persistence::traits::{MaintenancePersistence, StoragePruneResult};

#[async_trait]
impl MaintenancePersistence for DbPersistence {
    async fn prune_observability_storage(
        &self,
        max_bytes: u64,
        target_bytes: u64,
    ) -> anyhow::Result<Option<StoragePruneResult>> {
        ops::maintenance::prune_observability_storage(&self.conn, max_bytes, target_bytes).await
    }
}
