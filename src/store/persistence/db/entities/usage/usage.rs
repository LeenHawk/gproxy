//! `usages` table SeaORM entity (per-request usage row).
//!
//! `cost` is stored as the exact decimal string (TEXT) for lossless money
//! round-trips across backends.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "usages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Unique: usage rows are idempotent by `request_id` (§17).
    #[sea_orm(unique)]
    pub request_id: String,
    pub at: i64,
    pub route_name: Option<String>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: String,
    pub kind: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_5m_tokens: i64,
    pub cache_creation_1h_tokens: i64,
    #[sea_orm(column_type = "Text")]
    pub cost: String,
    /// §15.3: upstream latency (ms) of the settled attempt; 0 when unmeasured.
    #[sea_orm(default_value = 0)]
    pub latency_ms: i64,
    /// §17: `upstream` | `counted` | `estimated`.
    #[sea_orm(default_value = "")]
    pub usage_source: String,
    /// §17: `complete` | `interrupted`.
    #[sea_orm(default_value = "")]
    pub ended: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
