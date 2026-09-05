use crate::records::CredentialQuotaCycleRecord;
use crate::{Store, StoreError};

impl Store {
    pub(super) async fn with_models(
        &self,
        mut cycles: Vec<CredentialQuotaCycleRecord>,
    ) -> Result<Vec<CredentialQuotaCycleRecord>, StoreError> {
        for cycle in &mut cycles {
            super::metrics::hydrate(self, cycle).await?;
        }
        Ok(cycles)
    }
}
