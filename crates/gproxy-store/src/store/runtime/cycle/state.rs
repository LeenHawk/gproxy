use crate::StoreError;
use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, CycleTracking, QuotaBoundaryConfidence,
    QuotaBoundarySource,
};
use rust_decimal::Decimal;

/// Reset stamps wobble between observations: `…T06:59:59.999Z` and `…T07:00:00Z`
/// truncate a second apart, and upstream clocks drift against ours. Anything
/// inside this slack is the same boundary, not a new period.
const BOUNDARY_SLACK_SECONDS: i64 = 5;

/// Normalise, then check the store's input contract. Returns the observation
/// that should actually be recorded.
pub(super) fn settle(
    input: &CredentialQuotaObservation,
) -> Result<CredentialQuotaObservation, StoreError> {
    let mut input = input.clone();
    unstarted(&mut input);
    validate(&input)?;
    Ok(input)
}

/// A rolling window that has not been used yet reports its whole length as
/// remaining, so `reset_at - window` lands on the observation instant and walks
/// forward with every probe (Codex Spark does this on `/wham/usage` and in its
/// `x-…-reset-at` headers). Recorded usage is what proves a period has begun —
/// without it there is no boundary yet, only a window waiting to start.
fn unstarted(input: &mut CredentialQuotaObservation) {
    let unused = percent(
        input.used_percent,
        input.upstream_used,
        input.upstream_limit,
    )
    .is_some_and(|used| used.is_zero());
    let starts_now = input
        .period_start
        .is_some_and(|start| start >= input.observed_at - BOUNDARY_SLACK_SECONDS);
    if unused && starts_now {
        input.period_start = None;
        input.period_end = None;
        input.boundary_source = QuotaBoundarySource::Unknown;
        input.boundary_confidence = QuotaBoundaryConfidence::Unknown;
    }
}

/// Keep the boundary an open cycle already carries when the incoming one is the
/// same instant reported differently, so a one-second wobble does not close the
/// cycle and restart its accounting.
pub(super) fn hold_boundary(
    previous: &CredentialQuotaCycleRecord,
    next: &mut CredentialQuotaObservation,
) {
    if slack(previous.period_start, next.period_start) {
        next.period_start = previous.period_start;
    }
    if slack(previous.period_end, next.period_end) {
        next.period_end = previous.period_end;
    }
}

fn slack(previous: Option<i64>, next: Option<i64>) -> bool {
    previous.zip(next).is_some_and(|(previous, next)| {
        previous.abs_diff(next) <= BOUNDARY_SLACK_SECONDS.unsigned_abs()
    })
}

fn validate(input: &CredentialQuotaObservation) -> Result<(), StoreError> {
    let invalid = input.window_key.trim().is_empty()
        || input
            .period_start
            .zip(input.period_end)
            .is_some_and(|(start, end)| end <= start)
        || input
            .period_start
            .is_some_and(|start| start > input.observed_at)
        || input.upstream_used.is_some_and(|used| used < Decimal::ZERO)
        || input
            .upstream_limit
            .is_some_and(|limit| limit <= Decimal::ZERO)
        || input
            .used_percent
            .is_some_and(|percent| percent < Decimal::ZERO)
        || input.sample.started_at_ms > input.sample.received_at_ms;
    if invalid {
        return Err(StoreError::InvalidData {
            field: "quota observation",
            message: "invalid window, sample or counter".into(),
        });
    }
    Ok(())
}

pub(super) fn sample(input: &CredentialQuotaObservation) -> gproxy_core::QuotaSample {
    input.sample
}

pub(super) fn percent(
    percent: Option<Decimal>,
    used: Option<Decimal>,
    limit: Option<Decimal>,
) -> Option<Decimal> {
    percent.or_else(|| {
        used.zip(limit)
            .filter(|(_, limit)| *limit > Decimal::ZERO)
            .map(|(used, limit)| used / limit * Decimal::ONE_HUNDRED)
    })
}

pub(super) fn changed(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> bool {
    open.period_start
        .zip(next.period_start)
        .is_some_and(|(old, new)| old != new)
        || open
            .period_end
            .zip(next.period_end)
            .is_some_and(|(old, new)| old != new)
}

pub(super) fn adjusted(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> bool {
    open.upstream_limit
        .zip(next.upstream_limit)
        .is_some_and(|(old, new)| old != new)
        || (open.tracking.scope != next.scope
            || open.tracking.unit != next.unit
            || open.tracking.reset_behavior != next.reset_behavior)
}

pub(super) fn decreased(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> bool {
    if next.reset_behavior != gproxy_core::QuotaResetBehavior::Periodic || adjusted(open, next) {
        return false;
    }
    if next.unit.is_some()
        && let Some((old, new)) = open.upstream_used.zip(next.upstream_used)
    {
        return new < old;
    }
    percent(open.used_percent, open.upstream_used, open.upstream_limit)
        .zip(percent(
            next.used_percent,
            next.upstream_used,
            next.upstream_limit,
        ))
        .is_some_and(|(old, new)| new < old)
}

pub(super) fn tracking(input: &CredentialQuotaObservation, local_boundary: bool) -> CycleTracking {
    let sample = sample(input);
    CycleTracking {
        unit: input.unit.clone(),
        reset_behavior: input.reset_behavior,
        models: Default::default(),
        needs_rebuild: true,
        scope: input.scope.clone(),
        sample,
        baseline_at_ms: sample.received_at_ms,
        baseline_percent: percent(
            input.used_percent,
            input.upstream_used,
            input.upstream_limit,
        ),
        baseline_limit: input.upstream_limit,
        uncertain: false,
        local_boundary,
    }
}
