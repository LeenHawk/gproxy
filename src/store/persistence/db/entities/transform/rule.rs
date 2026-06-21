//! `rules` table SeaORM entity. One mutation rule within a [`super::rule_set`].
//! `config_json` / `filter_operation_keys` stored as text.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "rules")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rule_set_id: i64,
    pub kind: String,
    #[sea_orm(column_type = "Text")]
    pub config_json: String,
    pub filter_model_pattern: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub filter_operation_keys: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
