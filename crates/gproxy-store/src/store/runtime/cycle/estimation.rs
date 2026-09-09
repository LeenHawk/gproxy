use crate::StoreError;
use crate::records::{CredentialQuotaCycleRecord, CycleObservationRecord, UsageInput, UsageTotals};

pub(super) fn calculate(
    cycle: &CredentialQuotaCycleRecord,
    samples: &mut [CycleObservationRecord],
    usages: &[UsageInput],
    pending: &[(i64, String)],
) -> Result<(), StoreError> {
    let mut ordered = usages.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|usage| usage.upstream_started_at_ms);
    let mut total = UsageTotals::default();
    let mut session_attribution = false;
    let mut cursor = 0;
    let mut previous: Option<CycleObservationRecord> = None;
    let mut last_valid = None;
    for sample in samples {
        if previous.as_ref().is_none_or(|left| {
            left.baseline_at_ms != sample.baseline_at_ms
                || left.scope != sample.scope
                || left.upstream_limit != sample.upstream_limit
                || left.unit != sample.unit
        }) {
            total = UsageTotals::default();
            session_attribution = false;
            last_valid = None;
            cursor = ordered.partition_point(|usage| sent(usage) < sample.baseline_at_ms);
        }
        while let Some(usage) = ordered.get(cursor) {
            if sent(usage) >= sample.observed_at_ms {
                break;
            }
            cursor += 1;
            if !sample.scope.includes(&usage.upstream_model) {
                continue;
            }
            total.add(usage)?;
            session_attribution |= usage
                .dimensions
                .get("quota_attribution")
                .and_then(serde_json::Value::as_str)
                == Some("session");
        }
        let incomplete = cycle.tracking.needs_rebuild
            || session_attribution
            || pending.iter().any(|(at, model)| {
                *at >= sample.baseline_at_ms
                    && *at < sample.observed_at_ms
                    && sample.scope.includes(model)
            });
        let mut estimate = super::metrics::calculate(sample, &total, incomplete);
        if estimate.reason.is_none() {
            last_valid = Some(estimate.clone());
        } else if let Some(valid) = &last_valid {
            // Keep the original sample time and provenance when newer usage is
            // missing. A missing row is never treated as zero consumption.
            let reason = estimate.reason;
            estimate = valid.clone();
            estimate.reason = reason;
        }
        sample.estimate = Some(estimate);
        previous = Some(sample.clone());
    }
    Ok(())
}

fn sent(usage: &UsageInput) -> i64 {
    usage
        .upstream_started_at_ms
        .expect("cycle query selects send time")
}
