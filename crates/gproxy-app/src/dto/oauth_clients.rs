//! Registered public OAuth clients. This registry issues no client secrets,
//! so there is nothing secret in these shapes.

use gproxy_store::entity::oauth::client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OAuthClientDto {
    /// The public `client_id`, chosen at registration and immutable after it.
    pub id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub enabled: bool,
    /// Set by `retire`. A retired client is kept so session history still
    /// resolves; re-registering the same id must not un-revoke its grants.
    pub deleted_at_ms: Option<i64>,
}

impl From<client::Model> for OAuthClientDto {
    fn from(row: client::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            redirect_uris: super::allowlist(Some(row.redirect_uris)).unwrap_or_default(),
            enabled: row.enabled,
            deleted_at_ms: row.deleted_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OAuthClientWrite {
    /// Required, unlike every other family: the id *is* the `client_id` a
    /// third-party binary was built with, so it cannot be generated here.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OAuthClientPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub redirect_uris: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
