//! Downstream plans and their per-window allowances.

use super::double_option;
use gproxy_store::entity::subscription::{plan, plan_limit};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PlanDto {
    pub id: String,
    pub pool_id: String,
    pub name: String,
    /// Client-facing wire labels. They describe this gateway plan, not a
    /// purchased upstream entitlement.
    pub codex_plan_type: Option<String>,
    pub claude_subscription_type: Option<String>,
    pub claude_rate_limit_tier: Option<String>,
    pub enabled: bool,
}

impl From<plan::Model> for PlanDto {
    fn from(row: plan::Model) -> Self {
        Self {
            id: row.id,
            pool_id: row.pool_id,
            name: row.name,
            codex_plan_type: row.codex_plan_type,
            claude_subscription_type: row.claude_subscription_type,
            claude_rate_limit_tier: row.claude_rate_limit_tier,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PlanWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub pool_id: String,
    pub name: String,
    #[serde(default)]
    pub codex_plan_type: Option<String>,
    #[serde(default)]
    pub claude_subscription_type: Option<String>,
    #[serde(default)]
    pub claude_rate_limit_tier: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PlanPatch {
    #[serde(default)]
    pub pool_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub codex_plan_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub claude_subscription_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub claude_rate_limit_tier: Option<Option<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PlanLimitDto {
    pub id: String,
    pub plan_id: String,
    /// The client's stable window name, e.g. `primary` or `seven_day_sonnet`.
    pub window_key: String,
    /// USD, as a decimal string.
    pub limit: String,
    /// `total`, `fixed`, `day`, `week` or `month`, the same reset contract a
    /// quota uses.
    pub period: String,
    /// Required and positive for `fixed`; unused otherwise.
    pub period_seconds: Option<i64>,
    pub model_pattern: Option<String>,
}

impl From<plan_limit::Model> for PlanLimitDto {
    fn from(row: plan_limit::Model) -> Self {
        Self {
            id: row.id,
            plan_id: row.plan_id,
            window_key: row.window_key,
            limit: row.limit.to_string(),
            period: row.period,
            period_seconds: row.period_seconds,
            model_pattern: row.model_pattern,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PlanLimitWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub plan_id: String,
    pub window_key: String,
    pub limit: String,
    pub period: String,
    #[serde(default)]
    pub period_seconds: Option<i64>,
    #[serde(default)]
    pub model_pattern: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PlanLimitPatch {
    #[serde(default)]
    pub window_key: Option<String>,
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub period_seconds: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub model_pattern: Option<Option<String>>,
}
