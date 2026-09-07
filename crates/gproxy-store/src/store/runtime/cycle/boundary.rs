use crate::records::{CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaBoundarySource};

pub(super) fn transition(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
    changed: bool,
) -> (i64, bool) {
    let observed = super::state::sample(next).received_at_ms;
    if changed {
        if let Some(start) = next
            .period_start
            .map(|start| start * 1000)
            .filter(|start| Some(*start) != open.period_start.map(|start| start * 1000))
            .filter(|start| *start <= observed && *start > open.accounting_start_ms)
        {
            return (start, false);
        }
        if let Some(end) = open.accounting_end_ms.filter(|end| *end <= observed) {
            return (end, false);
        }
    }
    let discovered = open
        .tracking
        .pending_observation
        .as_ref()
        .filter(|pending| {
            !changed
                && !super::state::changed(open, pending)
                && !super::state::adjusted(open, pending)
                && super::state::decreased(open, pending)
        });
    (
        discovered.map_or(observed, |pending| pending.sample.received_at_ms),
        true,
    )
}

pub(super) fn trusted_reset(cycle: &CredentialQuotaCycleRecord) -> Option<i64> {
    (cycle.boundary_source == QuotaBoundarySource::Upstream)
        .then_some(cycle.period_end)
        .flatten()
}
