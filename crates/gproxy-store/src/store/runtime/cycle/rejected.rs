use crate::query::runtime;
use crate::records::{CredentialQuotaCycleRecord, CredentialQuotaObservation};
use crate::{Store, StoreError};

impl Store {
    pub(super) async fn record_rejected_quota(
        &self,
        cycle: &mut CredentialQuotaCycleRecord,
        raw: &CredentialQuotaObservation,
    ) -> Result<bool, StoreError> {
        let expected = cycle.version;
        cycle.version += 1;
        Ok(self
            .backend()
            .batch(vec![
                runtime::update_tracked_cycle(cycle, expected)?,
                runtime::insert_cycle_observation(cycle, raw, true)?,
            ])
            .await?[0]
            .affected_rows
            == 1)
    }
}
