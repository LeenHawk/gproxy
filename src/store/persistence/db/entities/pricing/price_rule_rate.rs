//! Independent metric rates attached to one model price rule.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "price_rule_rates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub price_rule_id: i64,
    pub metric: String,
    pub unit: String,
    pub unit_size: i64,
    #[sea_orm(column_type = "Text")]
    pub price_usd: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub conditions_json: Option<String>,
    pub sort_order: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
