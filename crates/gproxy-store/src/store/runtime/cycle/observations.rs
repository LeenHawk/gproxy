use crate::query::runtime;
use crate::records::{CredentialQuotaCycleRecord, CycleObservationRecord};
use crate::{Store, StoreError};

impl Store {
    pub async fn credential_quota_observations(
        &self,
        cycle: &CredentialQuotaCycleRecord,
        calculate: bool,
    ) -> Result<Vec<CycleObservationRecord>, StoreError> {
        let mut samples = self
            .backend()
            .execute(runtime::cycle_observations(cycle)?)
            .await?
            .rows
            .into_iter()
            .map(|row| {
                serde_json::from_str::<CycleObservationRecord>(row.text("snapshot_json")?)
                    .map_err(|error| StoreError::Database(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        samples.retain(|sample| !sample.rejected);
        if !calculate || samples.is_empty() {
            return Ok(samples);
        }
        let pending = self
            .backend()
            .execute(runtime::pending_cycle_usage(cycle)?)
            .await?
            .rows
            .into_iter()
            .map(|row| Ok((row.i64("started_at_ms")?, row.text("model")?.to_owned())))
            .collect::<Result<Vec<_>, StoreError>>()?;
        let mut usages = Vec::new();
        let mut after = 0;
        loop {
            let rows = self
                .backend()
                .execute(runtime::cycle_usage_rows(cycle, after, None)?)
                .await?
                .rows;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let record = crate::store::usage::parse_usage(row)?;
                after = record.id;
                usages.push(record.usage);
            }
        }
        super::estimation::calculate(cycle, &mut samples, &usages, &pending)?;
        Ok(samples)
    }
}
