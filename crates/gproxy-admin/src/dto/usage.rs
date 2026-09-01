use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum UsageGroupByDto {
    UserKey,
    User,
    Provider,
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UsageQueryDto {
    pub from: i64,
    pub to: i64,
    pub group_by: Option<UsageGroupByDto>,
    pub user_key_id: Option<i64>,
    pub user_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UsageStatisticsDto {
    pub user_key_id: Option<i64>,
    pub user_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub model: Option<String>,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_5m_tokens: u64,
    pub cache_creation_30m_tokens: u64,
    pub cache_creation_1h_tokens: u64,
    pub cost: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QuotaWindowDto {
    pub id: Option<i64>,
    pub quota_id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub window_kind: String,
    pub window_start: Option<i64>,
    pub reset_at: Option<i64>,
    pub started: bool,
    pub cost_used: String,
    pub cost_limit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum BoundarySourceDto {
    Upstream,
    Inferred,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum BoundaryConfidenceDto {
    Exact,
    Derived,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum QuotaCoverageDto {
    FullPeriodLowerBound,
    PartialLowerBound,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum QuotaCycleStatusDto {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum QuotaCycleCloseReasonDto {
    BoundaryCrossed,
    ManualReset,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CredentialQuotaCycleDto {
    pub id: i64,
    pub version: u64,
    pub credential_id: i64,
    pub window_key: String,
    /// Upstream display name for the limit, when the wire declared one.
    pub label: Option<String>,
    pub period_start: Option<i64>,
    pub period_end: Option<i64>,
    pub boundary_source: BoundarySourceDto,
    pub boundary_confidence: BoundaryConfidenceDto,
    pub status: QuotaCycleStatusDto,
    pub close_reason: Option<QuotaCycleCloseReasonDto>,
    pub last_observed_at: i64,
    pub upstream_used: Option<String>,
    pub upstream_limit: Option<String>,
    pub used_percent: Option<String>,
    pub coverage: QuotaCoverageDto,
    #[ts(type = "unknown")]
    pub metrics: Value,
    pub models: Vec<CredentialQuotaCycleModelDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CredentialQuotaCycleModelDto {
    pub model: String,
    #[ts(type = "unknown")]
    pub metrics: Value,
}
