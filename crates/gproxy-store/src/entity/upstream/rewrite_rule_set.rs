//! A reusable collection of payload rewrite rules.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "rewrite_rule_sets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub description: Option<String>,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[sea_orm(has_many)]
    pub rules: HasMany<super::rewrite_rule::Entity>,
    #[sea_orm(has_many)]
    pub provider_attachments: HasMany<super::provider_rewrite_rule_set::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
