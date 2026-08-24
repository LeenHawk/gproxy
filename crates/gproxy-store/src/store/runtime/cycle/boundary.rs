use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaBoundaryConfidence,
    QuotaBoundarySource,
};

pub(super) struct Boundary {
    pub at: i64,
    pub source: QuotaBoundarySource,
    pub confidence: QuotaBoundaryConfidence,
}

pub(super) fn resolve(
    open: &CredentialQuotaCycleRecord,
    next: &CredentialQuotaObservation,
) -> Option<Boundary> {
    let changed_start = open
        .period_start
        .zip(next.period_start)
        .filter(|(old, new)| old != new && *new <= next.observed_at)
        .map(|(_, at)| Boundary {
            at,
            source: next.boundary_source,
            confidence: next.boundary_confidence,
        });
    let old_end = open.period_end.map(|at| Boundary {
        at,
        source: open.boundary_source,
        confidence: open.boundary_confidence,
    });
    let chosen = match (changed_start, old_end) {
        (Some(start), Some(end)) if rank(&start) < rank(&end) => end,
        (Some(start), _) => start,
        (None, Some(end)) => end,
        (None, None) => Boundary {
            at: next.observed_at,
            source: next.boundary_source,
            confidence: next.boundary_confidence,
        },
    };
    (chosen.at <= next.observed_at).then_some(chosen)
}

pub(super) fn trusted_reset(cycle: &CredentialQuotaCycleRecord) -> Option<i64> {
    (cycle.boundary_source == QuotaBoundarySource::Upstream)
        .then_some(cycle.period_end)
        .flatten()
}

pub(super) fn provenance(next: &CredentialQuotaObservation) -> (u8, u8) {
    parts(next.boundary_source, next.boundary_confidence)
}

pub(super) fn record_provenance(open: &CredentialQuotaCycleRecord) -> (u8, u8) {
    parts(open.boundary_source, open.boundary_confidence)
}

fn rank(boundary: &Boundary) -> (u8, u8) {
    parts(boundary.source, boundary.confidence)
}

fn parts(source: QuotaBoundarySource, confidence: QuotaBoundaryConfidence) -> (u8, u8) {
    let source = match source {
        QuotaBoundarySource::Upstream => 2,
        QuotaBoundarySource::Inferred => 1,
        QuotaBoundarySource::Unknown => 0,
    };
    let confidence = match confidence {
        QuotaBoundaryConfidence::Exact => 3,
        QuotaBoundaryConfidence::Derived => 2,
        QuotaBoundaryConfidence::Partial => 1,
        QuotaBoundaryConfidence::Unknown => 0,
    };
    (source, confidence)
}
