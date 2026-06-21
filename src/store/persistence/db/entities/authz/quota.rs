//! `quotas` table SeaORM entity. Unique per `(scope, scope_id)`.
//!
//! `quota_total` / `cost_used` are stored as the exact decimal string (TEXT) so
//! money round-trips losslessly across SQLite/Postgres/MySQL.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "quotas")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub scope: String,
    pub scope_id: i64,
    #[sea_orm(column_type = "Text")]
    pub quota_total: String,
    #[sea_orm(column_type = "Text")]
    pub cost_used: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
