use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaBoundarySource {
    Upstream,
    Inferred,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaBoundaryConfidence {
    Exact,
    Derived,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaCycleStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaCycleCloseReason {
    BoundaryCrossed,
    ManualReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaCoverage {
    FullPeriodLowerBound,
    PartialLowerBound,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialQuotaObservation {
    pub credential_id: i64,
    pub window_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_end: Option<i64>,
    pub boundary_source: QuotaBoundarySource,
    pub boundary_confidence: QuotaBoundaryConfidence,
    pub observed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_used: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_limit: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<Decimal>,
    pub coverage: QuotaCoverage,
    pub metrics: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialQuotaCycleRecord {
    pub id: i64,
    pub credential_id: i64,
    pub window_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_end: Option<i64>,
    pub boundary_source: QuotaBoundarySource,
    pub boundary_confidence: QuotaBoundaryConfidence,
    pub status: QuotaCycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<QuotaCycleCloseReason>,
    pub last_observed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_used: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_limit: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<Decimal>,
    pub coverage: QuotaCoverage,
    pub metrics: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialQuotaPressure {
    pub credential_id: i64,
    pub used_percent: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_end: Option<i64>,
}
