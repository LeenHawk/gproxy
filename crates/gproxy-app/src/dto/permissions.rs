//! Permission rules: what one subject may ask of which provider, model and
//! operation.

use super::double_option;
use gproxy_store::entity::identity::permission;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PermissionDto {
    pub id: String,
    /// Exactly one of these two is set. A rule with neither is a grant to
    /// nobody; a rule with both is an intersection the snapshot still honours
    /// but this family refuses to write, because nobody who wrote one meant
    /// "the key *and* the user".
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    /// None means every provider.
    pub provider_id: Option<String>,
    /// A glob: `*` and `?`, anchored, case-sensitive. `*` matches a request
    /// that names no model at all.
    pub model_pattern: String,
    /// None means every operation.
    pub operation: Option<String>,
    /// `allow` or `deny`.
    pub action: String,
    /// Higher wins; ties fall back to id order.
    pub priority: i32,
}

impl From<permission::Model> for PermissionDto {
    fn from(row: permission::Model) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            provider_id: row.provider_id,
            model_pattern: row.model_pattern,
            operation: row.operation,
            action: row.action,
            priority: row.priority,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PermissionWrite {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub api_key_id: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Absent means `*`.
    #[serde(default)]
    pub model_pattern: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    pub action: String,
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct PermissionPatch {
    #[serde(default, deserialize_with = "double_option")]
    pub user_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub api_key_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub provider_id: Option<Option<String>>,
    #[serde(default)]
    pub model_pattern: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub operation: Option<Option<String>>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}
