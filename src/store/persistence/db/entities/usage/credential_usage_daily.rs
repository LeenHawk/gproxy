//! Permanent credential usage aggregate by UTC day and final upstream model.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_usage_daily")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub day_start: i64,
    pub credential_id: i64,
    pub provider_id: i64,
    pub model: Option<String>,
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
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
