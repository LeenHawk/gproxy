//! Configured rate limits. The live counters are in the cache; these are the
//! rows that say what to count and how far.

use super::double_option;
use gproxy_store::entity::limits::rate_limit;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RateLimitDto {
    pub id: String,
    /// The callers the limit covers. Neither set covers everyone; both set
    /// covers that user only when they use that key.
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    /// How the covered callers share the limit: `shared` (one counter for the
    /// rule), `user` (one per user) or `key` (one per API key; a caller
    /// without a key counts per user).
    pub scope: String,
    /// `requests`, `concurrency`, or any counter name the host charges.
    pub metric: String,
    /// A decimal string, never a JSON number: a limit has to survive a
    /// JavaScript `Number` unchanged.
    pub limit_value: String,
    pub period_seconds: i64,
    /// None means every model.
    pub model_pattern: Option<String>,
    pub enabled: bool,
}

impl From<rate_limit::Model> for RateLimitDto {
    fn from(row: rate_limit::Model) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            metric: row.metric,
            limit_value: row.limit_value.to_string(),
            period_seconds: row.period_seconds,
            model_pattern: row.model_pattern,
            scope: row.scope,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RateLimitWrite {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub api_key_id: Option<String>,
    pub metric: String,
    pub limit_value: String,
    pub period_seconds: i64,
    #[serde(default)]
    pub model_pattern: Option<String>,
    /// Absent means `shared`.
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RateLimitPatch {
    #[serde(default, deserialize_with = "double_option")]
    pub user_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub api_key_id: Option<Option<String>>,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub limit_value: Option<String>,
    #[serde(default)]
    pub period_seconds: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    pub model_pattern: Option<Option<String>>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
