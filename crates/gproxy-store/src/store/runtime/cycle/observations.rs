use crate::query::runtime;
use crate::records::{CredentialQuotaCycleRecord, CycleObservationRecord, UsageTotals};
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
        let mut totals = vec![UsageTotals::default(); samples.len()];
        let mut incomplete = samples
            .iter()
            .map(|sample| {
                cycle.tracking.needs_rebuild
                    || pending.iter().any(|(at, model)| {
                        *at < sample.observed_at_ms && sample.scope.includes(model)
                    })
            })
            .collect::<Vec<_>>();
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
                let usage = record.usage;
                let sent = usage
                    .upstream_started_at_ms
                    .expect("cycle query selects upstream send time");
                for (index, sample) in samples.iter().enumerate() {
                    if sent < sample.baseline_at_ms
                        || sent >= sample.observed_at_ms
                        || !sample.scope.includes(&usage.upstream_model)
                    {
                        continue;
                    }
                    totals[index].add(&usage)?;
                    incomplete[index] |= usage.ended != "complete"
                        || usage
                            .dimensions
                            .get("quota_attribution")
                            .and_then(serde_json::Value::as_str)
                            == Some("session");
                }
            }
        }
        for ((sample, total), incomplete) in samples.iter_mut().zip(totals).zip(incomplete) {
            sample.estimate = Some(super::metrics::calculate(sample, &total, incomplete));
        }
        Ok(samples)
    }
}
