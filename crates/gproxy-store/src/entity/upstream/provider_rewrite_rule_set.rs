//! Ordered attachment of a reusable rewrite rule set to a provider.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "provider_rewrite_rule_sets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed, unique_key = "provider_rule_set")]
    pub provider_id: String,
    #[sea_orm(indexed, unique_key = "provider_rule_set")]
    pub rule_set_id: String,
    /// Ascending attachment order within the provider; id breaks ties.
    #[sea_orm(default_value = 0)]
    pub sort_order: i64,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<super::provider::Entity>,
    #[sea_orm(belongs_to, from = "rule_set_id", to = "id", on_delete = "Cascade")]
    pub rule_set: BelongsTo<super::rewrite_rule_set::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
