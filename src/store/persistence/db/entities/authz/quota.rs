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
    #[sea_orm(column_type = "Text", nullable)]
    pub quota_daily: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub quota_weekly: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub quota_monthly: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub quota_5h: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub quota_7d: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub cost_used: String,
    #[sea_orm(column_type = "Text", default_value = "0")]
    pub day_used: String,
    #[sea_orm(default_value = 0)]
    pub day_anchor: i64,
    #[sea_orm(column_type = "Text", default_value = "0")]
    pub week_used: String,
    #[sea_orm(default_value = 0)]
    pub week_anchor: i64,
    #[sea_orm(column_type = "Text", default_value = "0")]
    pub month_used: String,
    #[sea_orm(default_value = 0)]
    pub month_anchor: i64,
    #[sea_orm(column_type = "Text")]
    pub five_hour_used: String,
    pub five_hour_anchor: i64,
    #[sea_orm(column_type = "Text")]
    pub seven_day_used: String,
    pub seven_day_anchor: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
