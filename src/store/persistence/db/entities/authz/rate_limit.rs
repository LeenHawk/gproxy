//! `rate_limits` table SeaORM entity.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "rate_limits")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub scope: String,
    pub scope_id: i64,
    pub route_pattern: String,
    pub rpm: Option<i64>,
    pub rpd: Option<i64>,
    pub total_tokens: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
