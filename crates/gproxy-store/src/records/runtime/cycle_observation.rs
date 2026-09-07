use gproxy_core::QuotaScope;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleObservationRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<super::CredentialQuotaObservation>,
    #[serde(default)]
    pub rejected: bool,
    pub started_at_ms: i64,
    pub observed_at_ms: i64,
    pub unit: Option<String>,
    pub upstream_used: Option<Decimal>,
    pub upstream_limit: Option<Decimal>,
    pub used_percent: Option<Decimal>,
    pub baseline_at_ms: i64,
    pub baseline_percent: Option<Decimal>,
    pub scope: QuotaScope,
    pub uncertain: bool,
    #[serde(skip)]
    pub estimate: Option<super::CycleEstimate>,
}

impl From<&super::CredentialQuotaCycleRecord> for CycleObservationRecord {
    fn from(cycle: &super::CredentialQuotaCycleRecord) -> Self {
        Self {
            raw: None,
            rejected: false,
            started_at_ms: cycle.tracking.sample.started_at_ms,
            observed_at_ms: cycle.tracking.sample.received_at_ms,
            unit: cycle.tracking.unit.clone(),
            upstream_used: cycle.upstream_used,
            upstream_limit: cycle.upstream_limit,
            used_percent: cycle.used_percent,
            baseline_at_ms: cycle.tracking.baseline_at_ms,
            baseline_percent: cycle.tracking.baseline_percent,
            scope: cycle.tracking.scope.clone(),
            uncertain: cycle.tracking.uncertain,
            estimate: None,
        }
    }
}
