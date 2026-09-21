//! Organizations: the outer scope that owns teams and shared credentials.

use super::{allowlist, double_option};
use gproxy_store::entity::identity::organization;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationDto {
    pub id: String,
    pub name: String,
    pub oauth_client_allowlist: Option<Vec<String>>,
    pub created_at_ms: i64,
}

impl From<organization::Model> for OrganizationDto {
    fn from(row: organization::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            oauth_client_allowlist: allowlist(row.oauth_client_allowlist),
            created_at_ms: row.created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub oauth_client_allowlist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub oauth_client_allowlist: Option<Option<Vec<String>>>,
}
