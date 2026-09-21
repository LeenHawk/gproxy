//! Teams: an inner scope inside exactly one organization. A team cannot be
//! moved between organizations — its credentials, budgets and bound keys were
//! all scoped under the parent, and re-parenting would silently re-scope them.

use super::{allowlist, double_option};
use gproxy_store::entity::identity::team;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct TeamDto {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub oauth_client_allowlist: Option<Vec<String>>,
    pub created_at_ms: i64,
}

impl From<team::Model> for TeamDto {
    fn from(row: team::Model) -> Self {
        Self {
            id: row.id,
            organization_id: row.organization_id,
            name: row.name,
            oauth_client_allowlist: allowlist(row.oauth_client_allowlist),
            created_at_ms: row.created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct TeamWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub organization_id: String,
    pub name: String,
    #[serde(default)]
    pub oauth_client_allowlist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct TeamPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub oauth_client_allowlist: Option<Option<Vec<String>>>,
}
