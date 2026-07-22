use crate::store::persistence::traits::{MaintenancePersistence, StoragePruneResult};

use super::super::LibsqlPersistence;

#[async_trait::async_trait(?Send)]
impl MaintenancePersistence for LibsqlPersistence {
    async fn prune_observability_storage(
        &self,
        _max_bytes: u64,
        _target_bytes: u64,
    ) -> anyhow::Result<Option<StoragePruneResult>> {
        Ok(None)
    }
}
