use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::QuotaScope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum QuotaKind {
    Balance,
    Budget,
    Window,
    RateLimit,
    UsageReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum QuotaQueryMode {
    Probe,
    Response,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum QuotaSupport {
    Ready,
    RequiresAuthorization,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaSource {
    pub id: String,
    pub label: String,
    pub kinds: Vec<QuotaKind>,
    pub mode: QuotaQueryMode,
    pub support: QuotaSupport,
    pub reason: Option<String>,
    pub automatic: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum QuotaSubject {
    Account,
    Organization,
    Project,
    Key,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum QuotaAvailability {
    Available,
    Unavailable,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaComponent {
    pub kind: String,
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaBalance {
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub remaining: Option<Decimal>,
    pub unit: Option<String>,
    pub availability: QuotaAvailability,
    pub components: Vec<QuotaComponent>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaAllowance {
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub used: Option<Decimal>,
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub limit: Option<Decimal>,
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub remaining: Option<Decimal>,
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub used_percent: Option<Decimal>,
    pub unlimited: bool,
    pub unit: Option<String>,
    pub period_start: Option<i64>,
    pub period_end: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaUsageReport {
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub used: Decimal,
    pub unit: String,
    pub period_start: i64,
    pub period_end: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuotaValue {
    Balance(QuotaBalance),
    Budget(QuotaAllowance),
    Window(QuotaAllowance),
    RateLimit(QuotaAllowance),
    UsageReport(QuotaUsageReport),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaEntry {
    pub id: String,
    pub source_id: String,
    pub label: Option<String>,
    pub subject: QuotaSubject,
    pub model_scope: QuotaScope,
    pub observed_at_ms: i64,
    pub value: QuotaValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaRefreshError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaSourceState {
    pub capability: QuotaSource,
    pub attempted_at_ms: Option<i64>,
    pub observed_at_ms: Option<i64>,
    pub error: Option<QuotaRefreshError>,
    pub reset_credits: Option<crate::QuotaResetCredits>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub struct QuotaSnapshot {
    pub sources: Vec<QuotaSourceState>,
    pub entries: Vec<QuotaEntry>,
}

impl QuotaEntry {
    pub fn from_window(observation: &crate::QuotaObservation, observed_at_ms: i64) -> Self {
        Self {
            id: observation.window_key.clone(),
            source_id: "subscription".into(),
            label: observation.label.clone(),
            subject: QuotaSubject::Account,
            model_scope: observation.scope.clone(),
            observed_at_ms,
            value: QuotaValue::Window(QuotaAllowance {
                used: observation.upstream_used,
                limit: observation.upstream_limit,
                remaining: observation
                    .upstream_limit
                    .zip(observation.upstream_used)
                    .map(|(limit, used)| limit - used),
                used_percent: observation.used_percent,
                unlimited: false,
                unit: observation.unit.clone(),
                period_start: observation.period_start,
                period_end: observation.period_end,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuotaSourcePage {
    pub entries: Vec<QuotaEntry>,
    pub next_cursor: Option<String>,
}
