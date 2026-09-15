//! Historical quota windows. quota_id is a historical reference, not a cascading configuration FK.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "quota_windows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "window")]
    pub quota_id: String,
    #[sea_orm(unique_key = "window")]
    pub starts_at_ms: i64,
    pub ends_at_ms: Option<i64>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub used: Decimal,
    pub quota_snapshot: Json,
    #[sea_orm(has_many)]
    pub settlements: HasMany<super::quota_settlement::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
