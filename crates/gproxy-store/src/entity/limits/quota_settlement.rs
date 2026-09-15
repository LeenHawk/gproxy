//! A request contribution to a quota window. Independent of removable usage-detail records.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "quota_settlements")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub window_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub request_id: String,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub amount: Decimal,
    pub settled_at_ms: i64,
    #[sea_orm(belongs_to, from = "window_id", to = "id", on_delete = "Cascade")]
    pub window: BelongsTo<super::quota_window::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
