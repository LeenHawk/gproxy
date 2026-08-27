use crate::query::runtime;
use crate::records::{CredentialQuotaCycleModelRecord, CredentialQuotaCycleRecord};
use crate::{Store, StoreError};

impl Store {
    pub(super) async fn with_models(
        &self,
        mut cycles: Vec<CredentialQuotaCycleRecord>,
    ) -> Result<Vec<CredentialQuotaCycleRecord>, StoreError> {
        if cycles.is_empty() {
            return Ok(cycles);
        }
        let ids = cycles.iter().map(|cycle| cycle.id).collect::<Vec<_>>();
        let rows = self
            .backend()
            .execute(runtime::select_credential_cycle_models(&ids)?)
            .await?
            .rows;
        let mut by_cycle = std::collections::BTreeMap::<i64, Vec<_>>::new();
        for row in rows {
            let cycle_id = row.i64("cycle_id")?;
            let metrics = serde_json::from_str(row.text("metrics_json")?).map_err(|error| {
                StoreError::InvalidData {
                    field: "credential cycle model metrics",
                    message: error.to_string(),
                }
            })?;
            by_cycle
                .entry(cycle_id)
                .or_default()
                .push(CredentialQuotaCycleModelRecord {
                    model: row.text("model")?.to_owned(),
                    metrics,
                });
        }
        for cycle in &mut cycles {
            cycle.models = by_cycle.remove(&cycle.id).unwrap_or_default();
        }
        Ok(cycles)
    }
}
