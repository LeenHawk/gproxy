//! Capacity pools and the upstream subscriptions that back them.

use gproxy_store::entity::subscription::{pool, pool_member};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolDto {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub created_at_ms: i64,
}

impl From<pool::Model> for PoolDto {
    fn from(row: pool::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            enabled: row.enabled,
            created_at_ms: row.created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolMemberDto {
    pub id: String,
    pub pool_id: String,
    pub credential_id: String,
    /// The canonical issuer plus upstream subscription identity, which is what
    /// deduplicates one real subscription imported under several credentials.
    pub source_key: String,
    pub enabled: bool,
}

impl From<pool_member::Model> for PoolMemberDto {
    fn from(row: pool_member::Model) -> Self {
        Self {
            id: row.id,
            pool_id: row.pool_id,
            credential_id: row.credential_id,
            source_key: row.source_key,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolMemberWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub pool_id: String,
    pub credential_id: String,
    pub source_key: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolMemberPatch {
    #[serde(default)]
    pub pool_id: Option<String>,
    #[serde(default)]
    pub credential_id: Option<String>,
    #[serde(default)]
    pub source_key: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
