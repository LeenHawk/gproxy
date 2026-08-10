//! Permanent upstream quota-window cycles.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_quota_cycles")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub credential_id: i64,
    pub provider_id: i64,
    pub channel: String,
    pub window_key: String,
    pub name: String,
    pub label: Option<String>,
    pub scope_kind: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub scope_json: Option<String>,
    pub meter_kind: String,
    pub period_start: Option<i64>,
    pub period_end: Option<i64>,
    pub boundary_source: String,
    pub boundary_confidence: String,
    pub close_reason: Option<String>,
    pub status: String,
    /// `1` for an open row and NULL after finalization. A unique index over
    /// `(credential_id, window_key, open_slot)` permits many finalized rows
    /// while enforcing at most one open row on every supported SQL dialect.
    pub open_slot: Option<i64>,
    pub last_observed_at: Option<i64>,
    #[sea_orm(column_type = "Text", nullable)]
    pub used_percent: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub upstream_used: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub upstream_limit: Option<String>,
    pub coverage: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub image_output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_5m_tokens: i64,
    pub cache_creation_30m_tokens: i64,
    pub cache_creation_1h_tokens: i64,
    #[sea_orm(column_type = "Text")]
    pub cost: String,
    pub estimated_tokens: Option<i64>,
    #[sea_orm(column_type = "Text", nullable)]
    pub estimated_cost: Option<String>,
    pub aggregated_through: Option<i64>,
    pub finalized_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
