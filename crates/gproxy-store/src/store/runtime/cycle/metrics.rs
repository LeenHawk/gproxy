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
    cycle.models = tracking
        .models
        .iter()
        .map(|(model, metrics)| CredentialQuotaCycleModelRecord {
            model: model.clone(),
            metrics: metrics.clone(),
        })
        .collect();
    let samples = store.credential_quota_observations(cycle, true).await?;
    cycle.estimate = samples.last().and_then(|sample| sample.estimate.clone());
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
