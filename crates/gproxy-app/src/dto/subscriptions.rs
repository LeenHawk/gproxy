//! One user's issued subscription to a plan.

use super::double_option;
use gproxy_store::entity::subscription::user_subscription;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct SubscriptionDto {
    pub id: String,
    pub user_id: String,
    pub plan_id: String,
    pub enabled: bool,
    pub created_at_ms: i64,
    /// The anchor fixed-duration allowance windows are measured from.
    pub starts_at_ms: i64,
    pub expires_at_ms: Option<i64>,
}

impl From<user_subscription::Model> for SubscriptionDto {
    fn from(row: user_subscription::Model) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            plan_id: row.plan_id,
            enabled: row.enabled,
            created_at_ms: row.created_at_ms,
            starts_at_ms: row.starts_at_ms,
            expires_at_ms: row.expires_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct SubscriptionWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub user_id: String,
    pub plan_id: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Absent anchors the subscription at the moment it is issued.
    #[serde(default)]
    pub starts_at_ms: Option<i64>,
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct SubscriptionPatch {
    /// Re-pointing an issued subscription at another plan is allowed but
    /// deliberate: the plan's limits are copied at issuance and existing
    /// windows are not resized by this.
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub starts_at_ms: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    pub expires_at_ms: Option<Option<i64>>,
}
