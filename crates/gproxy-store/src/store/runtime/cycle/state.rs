use rust_decimal::Decimal;

use crate::StoreError;
use crate::records::{CredentialQuotaCycleRecord, CredentialQuotaObservation};

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
    if !input.metrics.is_object() {
        return Err(invalid("metrics", "must be an object"));
    }
    Ok(())
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

pub(super) fn boundary(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> i64 {
    next.period_start
        .or(open.period_end)
        .unwrap_or(next.observed_at)
        .min(next.observed_at)
}

fn invalid(field: &'static str, message: &'static str) -> StoreError {
    StoreError::InvalidData {
        field,
        message: message.into(),
    }
}
