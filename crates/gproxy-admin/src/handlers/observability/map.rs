use gproxy_store::records::{
    CredentialQuotaCycleRecord, QuotaBoundaryConfidence, QuotaBoundarySource, QuotaCoverage,
    QuotaCycleStatus, QuotaRecord, QuotaWindowKind, QuotaWindowRecord,
};

use crate::dto::{
    BoundaryConfidenceDto, BoundarySourceDto, CredentialQuotaCycleDto, QuotaCoverageDto,
    QuotaCycleCloseReasonDto, QuotaCycleStatusDto, QuotaWindowDto,
};

pub(crate) fn configured_windows(quota: &QuotaRecord) -> Vec<QuotaWindowKind> {
    [
        (QuotaWindowKind::Total, Some(quota.quota_total)),
        (QuotaWindowKind::Daily, quota.quota_daily),
        (QuotaWindowKind::Weekly, quota.quota_weekly),
        (QuotaWindowKind::Monthly, quota.quota_monthly),
        (QuotaWindowKind::FiveHour, quota.quota_5h),
        (QuotaWindowKind::SevenDay, quota.quota_7d),
    ]
    .into_iter()
    .filter_map(|(kind, limit)| limit.map(|_| kind))
    .collect()
}

pub(crate) fn quota_window(
    quota: &QuotaRecord,
    window: &QuotaWindowRecord,
) -> Option<QuotaWindowDto> {
    let limit = limit(quota, window.window_kind)?;
    Some(QuotaWindowDto {
        id: Some(window.id),
        quota_id: quota.id,
        subject_kind: quota.subject_kind.clone(),
        subject_id: quota.subject_id,
        window_kind: window.window_kind.as_str().into(),
        window_start: Some(window.window_start),
        reset_at: window.reset_at,
        started: true,
        cost_used: decimal(window.cost_used),
        cost_limit: decimal(limit),
    })
}

pub(crate) fn unstarted_window(
    quota: &QuotaRecord,
    kind: QuotaWindowKind,
    now: i64,
) -> Option<QuotaWindowDto> {
    let limit = limit(quota, kind)?;
    let period = gproxy_store::Store::quota_window_period(kind, now);
    Some(QuotaWindowDto {
        id: None,
        quota_id: quota.id,
        subject_kind: quota.subject_kind.clone(),
        subject_id: quota.subject_id,
        window_kind: kind.as_str().into(),
        window_start: period.map(|(start, _)| start),
        reset_at: period.and_then(|(_, reset)| reset),
        started: period.is_some(),
        cost_used: "0".into(),
        cost_limit: decimal(limit),
    })
}

fn limit(quota: &QuotaRecord, kind: QuotaWindowKind) -> Option<rust_decimal::Decimal> {
    match kind {
        QuotaWindowKind::Total => Some(quota.quota_total),
        QuotaWindowKind::Daily => quota.quota_daily,
        QuotaWindowKind::Weekly => quota.quota_weekly,
        QuotaWindowKind::Monthly => quota.quota_monthly,
        QuotaWindowKind::FiveHour => quota.quota_5h,
        QuotaWindowKind::SevenDay => quota.quota_7d,
    }
}

pub(super) fn credential_cycle(value: &CredentialQuotaCycleRecord) -> CredentialQuotaCycleDto {
    CredentialQuotaCycleDto {
        id: value.id,
        version: value.version,
        credential_id: value.credential_id,
        window_key: value.window_key.clone(),
        period_start: value.period_start,
        period_end: value.period_end,
        boundary_source: match value.boundary_source {
            QuotaBoundarySource::Upstream => BoundarySourceDto::Upstream,
            QuotaBoundarySource::Inferred => BoundarySourceDto::Inferred,
            QuotaBoundarySource::Unknown => BoundarySourceDto::Unknown,
        },
        boundary_confidence: match value.boundary_confidence {
            QuotaBoundaryConfidence::Exact => BoundaryConfidenceDto::Exact,
            QuotaBoundaryConfidence::Derived => BoundaryConfidenceDto::Derived,
            QuotaBoundaryConfidence::Partial => BoundaryConfidenceDto::Partial,
            QuotaBoundaryConfidence::Unknown => BoundaryConfidenceDto::Unknown,
        },
        status: match value.status {
            QuotaCycleStatus::Open => QuotaCycleStatusDto::Open,
            QuotaCycleStatus::Closed => QuotaCycleStatusDto::Closed,
        },
        close_reason: value.close_reason.map(|reason| match reason {
            gproxy_store::records::QuotaCycleCloseReason::BoundaryCrossed => {
                QuotaCycleCloseReasonDto::BoundaryCrossed
            }
            gproxy_store::records::QuotaCycleCloseReason::ManualReset => {
                QuotaCycleCloseReasonDto::ManualReset
            }
        }),
        last_observed_at: value.last_observed_at,
        upstream_used: value.upstream_used.map(decimal),
        upstream_limit: value.upstream_limit.map(decimal),
        used_percent: value.used_percent.map(decimal),
        coverage: match value.coverage {
            QuotaCoverage::FullPeriodLowerBound => QuotaCoverageDto::FullPeriodLowerBound,
            QuotaCoverage::PartialLowerBound => QuotaCoverageDto::PartialLowerBound,
            QuotaCoverage::Unknown => QuotaCoverageDto::Unknown,
        },
        metrics: value.metrics.clone(),
    }
}

fn decimal(value: rust_decimal::Decimal) -> String {
    value.normalize().to_string()
}
