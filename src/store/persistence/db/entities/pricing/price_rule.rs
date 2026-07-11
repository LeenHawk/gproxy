//! `price_rules` table SeaORM entity. Decimal prices are stored as text.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "price_rules")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub provider_id: Option<i64>,
    pub match_type: String,
    pub model_match: String,
    #[sea_orm(column_type = "Text")]
    pub input_price: String,
    #[sea_orm(column_type = "Text")]
    pub output_price: String,
    #[sea_orm(column_type = "Text")]
    pub cache_read_price: String,
    #[sea_orm(column_type = "Text")]
    pub cache_creation_5m_price: String,
    #[sea_orm(column_type = "Text")]
    pub cache_creation_30m_price: String,
    #[sea_orm(column_type = "Text")]
    pub cache_creation_1h_price: String,
    #[sea_orm(column_type = "Text")]
    pub image_price: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
