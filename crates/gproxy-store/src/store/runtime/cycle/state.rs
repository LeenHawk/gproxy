use rust_decimal::Decimal;

use super::boundary::{provenance, record_provenance};
use crate::StoreError;
use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaCoverage, QuotaCycleCloseReason,
};

pub(super) fn validate(input: &CredentialQuotaObservation) -> Result<(), StoreError> {
    if input.window_key.trim().is_empty() {
        return Err(invalid("window_key", "must not be empty"));
    }
    if input
        .period_start
        .zip(input.period_end)
        .is_some_and(|(start, end)| end <= start)
    {
        return Err(invalid("period_end", "must be after period_start"));
    }
    if input
        .period_start
        .is_some_and(|start| start > input.observed_at)
    {
        return Err(invalid("period_start", "must not follow observed_at"));
    }
    if input
        .upstream_used
        .is_some_and(|value| value < Decimal::ZERO)
    {
        return Err(invalid("upstream_used", "must not be negative"));
    }
    if input
        .upstream_limit
        .is_some_and(|value| value <= Decimal::ZERO)
    {
        return Err(invalid("upstream_limit", "must be positive"));
    }
    if input
        .used_percent
        .is_some_and(|value| value < Decimal::ZERO)
    {
        return Err(invalid("used_percent", "must not be negative"));
    }
    Ok(())
}

pub(super) fn stale_open(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> bool {
    if next.observed_at < open.last_observed_at {
        return true;
    }
    if open
        .period_start
        .zip(next.period_start)
        .is_some_and(|(old, new)| new < old)
    {
        return true;
    }
    next.observed_at == open.last_observed_at
        && open
            .period_end
            .zip(next.period_end)
            .is_some_and(|(old, new)| new < old && provenance(next) <= record_provenance(open))
}

pub(super) fn preserve_cycle_bounds(
    open: &CredentialQuotaCycleRecord,
    next: &mut CredentialQuotaObservation,
) {
    if next.period_start.is_none() {
        next.period_start = open.period_start;
    }
    if next.period_end.is_none() && next.period_start == open.period_start {
        next.period_end = open.period_end;
    }
    if (next.period_start, next.period_end) == (open.period_start, open.period_end)
        && record_provenance(open) > provenance(next)
    {
        next.boundary_source = open.boundary_source;
        next.boundary_confidence = open.boundary_confidence;
    }
}

pub(super) fn retain_cycle_boundary(
    open: &CredentialQuotaCycleRecord,
    next: &mut CredentialQuotaObservation,
) {
    next.period_start = open.period_start;
    next.period_end = open.period_end;
    next.boundary_source = open.boundary_source;
    next.boundary_confidence = open.boundary_confidence;
}

pub(super) fn merge_same_second(
    open: &CredentialQuotaCycleRecord,
    next: &mut CredentialQuotaObservation,
) {
    if next.observed_at != open.last_observed_at {
        return;
    }
    let previous = reported_percent(open.used_percent, open.upstream_used, open.upstream_limit);
    let incoming = reported_percent(next.used_percent, next.upstream_used, next.upstream_limit);
    if previous > incoming {
        next.used_percent = open.used_percent;
        next.upstream_used = open.upstream_used;
        next.upstream_limit = open.upstream_limit;
    }
    if record_provenance(open) > provenance(next) {
        next.period_start = open.period_start;
        next.period_end = open.period_end;
        next.boundary_source = open.boundary_source;
        next.boundary_confidence = open.boundary_confidence;
    }
}

pub(super) fn stale_after_close(
    closed: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> bool {
    let barrier = closed.period_end.map_or(closed.last_observed_at, |end| {
        end.max(closed.last_observed_at)
    });
    next.period_start
        .zip(closed.period_end)
        .is_some_and(|(start, end)| start < end)
        || (closed.period_end.is_some() && next.period_end == closed.period_end)
        || next.observed_at < barrier
        || (closed.close_reason == Some(QuotaCycleCloseReason::ManualReset)
            && next.observed_at == barrier)
}

pub(super) fn continue_after_natural_close(
    closed: &CredentialQuotaCycleRecord,
    next: &mut CredentialQuotaObservation,
) {
    let Some(boundary) = closed.period_end else {
        return;
    };
    if closed.close_reason == Some(QuotaCycleCloseReason::BoundaryCrossed)
        && next.observed_at >= boundary
        && next.period_end != Some(boundary)
        && next.period_start.is_none_or(|start| start < boundary)
    {
        next.period_start = Some(boundary);
    }
}

pub(super) fn crossed_boundary(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> bool {
    let start_changed = open
        .period_start
        .zip(next.period_start)
        .is_some_and(|(old, new)| old != new && new <= next.observed_at);
    let ended_then_changed = open
        .period_end
        .zip(next.period_end)
        .is_some_and(|(old, new)| old != new && next.observed_at >= old);
    start_changed || ended_then_changed
}

pub(super) fn update_coverage(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> QuotaCoverage {
    if next.period_start.is_none() {
        QuotaCoverage::Unknown
    } else {
        open.coverage
    }
}

pub(super) fn new_coverage(
    latest: Option<&CredentialQuotaCycleRecord>,
    next: &CredentialQuotaObservation,
) -> QuotaCoverage {
    let Some(start) = next.period_start else {
        return QuotaCoverage::Unknown;
    };
    if latest.is_some_and(|cycle| {
        cycle.close_reason == Some(QuotaCycleCloseReason::BoundaryCrossed)
            && cycle.period_end == Some(start)
    }) {
        QuotaCoverage::FullPeriodLowerBound
    } else {
        QuotaCoverage::PartialLowerBound
    }
}

fn invalid(field: &'static str, message: &'static str) -> StoreError {
    StoreError::InvalidData {
        field,
        message: message.into(),
    }
}

fn reported_percent(
    percent: Option<Decimal>,
    used: Option<Decimal>,
    limit: Option<Decimal>,
) -> Option<Decimal> {
    percent.or_else(|| {
        let used = used?;
        let limit = limit?;
        (limit > Decimal::ZERO).then(|| used / limit * Decimal::ONE_HUNDRED)
    })
}
