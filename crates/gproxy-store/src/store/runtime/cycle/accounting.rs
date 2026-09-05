use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::query::runtime;
use crate::records::{CredentialQuotaCycleRecord, UsageInput, UsageTotals};
use crate::{Store, StoreError};

pub(super) fn increment(metrics: &mut Value, usage: &UsageInput) -> Result<(), StoreError> {
    let mut totals = UsageTotals::default();
    totals.add(usage)?;
    let delta = super::metrics::metrics(&totals);
    let mut values: BTreeMap<String, Decimal> = serde_json::from_value(metrics.clone())
        .map_err(|error| StoreError::Database(error.to_string()))?;
    let delta: BTreeMap<String, Decimal> =
        serde_json::from_value(delta).map_err(|error| StoreError::Database(error.to_string()))?;
    for (name, amount) in delta {
        *values.entry(name).or_default() += amount;
    }
    *metrics = serde_json::to_value(values).expect("decimal metrics serialize");
    Ok(())
}

impl Store {
    pub(super) async fn rebuild_cycle(&self, id: i64) -> Result<(), StoreError> {
        for _ in 0..8 {
            let Some(mut cycle) = self.credential_quota_cycle(id).await? else {
                return Ok(());
            };
            let tracking = &mut cycle.tracking;
            if !tracking.needs_rebuild {
                return Ok(());
            }
            tracking.models.clear();
            cycle.metrics = if tracking.scope == gproxy_core::QuotaScope::Unknown {
                serde_json::json!({})
            } else {
                super::metrics::metrics(&UsageTotals::default())
            };
            let expected = cycle.version;
            let mut after = 0;
            let mut statements = Vec::new();
            loop {
                let rows = self
                    .backend()
                    .execute(runtime::cycle_usage_rows(&cycle, after, None)?)
                    .await?
                    .rows;
                if rows.is_empty() {
                    break;
                }
                for row in rows {
                    let record = crate::store::usage::parse_usage(row)?;
                    after = record.id;
                    if cycle.tracking.scope.includes(&record.usage.upstream_model) {
                        accumulate(&mut cycle, &record.usage)?;
                        statements.push(runtime::link_cycle_usage(&cycle, record.id)?);
                    }
                }
            }
            cycle.tracking.needs_rebuild = false;
            cycle.version += 1;
            statements.push(runtime::update_tracked_cycle(&cycle, expected)?);
            if self
                .backend()
                .batch(statements)
                .await?
                .last()
                .is_some_and(|result| result.affected_rows == 1)
            {
                return Ok(());
            }
        }
        Err(StoreError::Database(
            "cycle rebuild remained contended".into(),
        ))
    }
}

pub(super) fn accumulate(
    cycle: &mut CredentialQuotaCycleRecord,
    usage: &UsageInput,
) -> Result<(), StoreError> {
    increment(&mut cycle.metrics, usage)?;
    let model = cycle
        .tracking
        .models
        .entry(usage.upstream_model.clone())
        .or_insert_with(|| serde_json::json!({}));
    increment(model, usage)
}
