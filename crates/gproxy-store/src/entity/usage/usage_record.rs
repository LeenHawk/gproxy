//! Per-request usage summary. Identity fields are historical references, without configuration FKs.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "usage_records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub request_id: String,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    pub model: String,
    pub operation: String,
    pub dialect: String,
    pub metrics: Json,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub cost: Option<Decimal>,
    pub pricing_snapshot: Option<Json>,
    #[sea_orm(default_value = false)]
    pub estimated: bool,
    #[sea_orm(indexed)]
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
