use crate::query::runtime;
use crate::records::{
    CredentialQuotaCycleModelRecord, CredentialQuotaCycleRecord, CycleEstimate,
    CycleObservationRecord, UsageTotals,
};
use crate::{Store, StoreError};
use rust_decimal::Decimal;
use serde_json::Value;

pub(super) async fn hydrate(
    store: &Store,
    cycle: &mut CredentialQuotaCycleRecord,
) -> Result<(), StoreError> {
    let tracking = &cycle.tracking;
    if tracking.scope == gproxy_core::QuotaScope::Unknown {
        cycle.metrics = serde_json::json!({});
        cycle.models.clear();
        cycle.estimate = Some(unavailable(
            "unknown_scope",
            &CycleObservationRecord::from(&*cycle),
        ));
        return Ok(());
    }
    let mut delta = UsageTotals::default();
    let mut after = 0;
    let pending = store
        .backend()
        .execute(runtime::incomplete_cycle_usage(cycle)?)
        .await?
        .rows;
    let mut incomplete = tracking.needs_rebuild || !pending.is_empty();
    loop {
        let rows = store
            .backend()
            .execute(runtime::cycle_usage_rows(cycle, after, Some(false))?)
            .await?
            .rows;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let record = crate::store::usage::parse_usage(row)?;
            after = record.id;
            let usage = record.usage;
            if !tracking.scope.includes(&usage.upstream_model) {
                continue;
            }
            let sent = usage
                .upstream_started_at_ms
                .expect("cycle query selects rows with an upstream send time");
            if sent >= tracking.baseline_at_ms && sent < tracking.sample.received_at_ms {
                delta.add(&usage)?;
                incomplete |= usage.ended != "complete"
                    || usage
                        .dimensions
                        .get("quota_attribution")
                        .and_then(Value::as_str)
                        == Some("session");
            }
        }
    }
    cycle.models = tracking
        .models
        .iter()
        .map(|(model, metrics)| CredentialQuotaCycleModelRecord {
            model: model.clone(),
            metrics: metrics.clone(),
        })
        .collect();
    cycle.estimate = Some(calculate(
        &CycleObservationRecord::from(&*cycle),
        &delta,
        incomplete,
    ));
    Ok(())
}

pub(super) fn calculate(
    sample: &CycleObservationRecord,
    delta: &UsageTotals,
    incomplete: bool,
) -> CycleEstimate {
    let current = super::state::percent(
        sample.used_percent,
        sample.upstream_used,
        sample.upstream_limit,
    );
    let growth = current
        .zip(sample.baseline_percent)
        .map(|(current, baseline)| current - baseline);
    if sample.scope == gproxy_core::QuotaScope::Unknown {
        unavailable("unknown_scope", sample)
    } else if sample.uncertain {
        unavailable("unordered_observations", sample)
    } else if incomplete {
        unavailable("incomplete_usage", sample)
    } else if delta.requests == 0 || growth.is_none_or(|growth| growth < Decimal::ONE) {
        unavailable("insufficient_samples", sample)
    } else {
        let factor = Decimal::ONE_HUNDRED / growth.expect("positive growth");
        CycleEstimate {
            tokens: Some(delta.total_tokens() * factor),
            cost: Some(delta.cost * factor),
            reason: None,
            from_ms: Some(sample.baseline_at_ms),
            to_ms: Some(sample.observed_at_ms),
        }
    }
}

fn unavailable(reason: &str, sample: &CycleObservationRecord) -> CycleEstimate {
    CycleEstimate {
        tokens: None,
        cost: None,
        reason: Some(reason.into()),
        from_ms: Some(sample.baseline_at_ms),
        to_ms: Some(sample.observed_at_ms),
    }
}

pub(super) fn metrics(totals: &UsageTotals) -> Value {
    let mut metrics = totals.metrics.clone();
    metrics.extend([
        ("requests".into(), Decimal::from(totals.requests)),
        ("input_tokens".into(), Decimal::from(totals.input_tokens)),
        ("output_tokens".into(), Decimal::from(totals.output_tokens)),
        (
            "cached_input_tokens".into(),
            Decimal::from(totals.cached_input_tokens),
        ),
        ("total_tokens".into(), totals.total_tokens()),
        ("cost".into(), totals.cost),
    ]);
    serde_json::to_value(metrics).expect("decimal metrics serialize")
}
