//! Model/operation pricing selection; a null provider applies globally.
//! Provider rules precede global rules; within each scope lower (priority, id) wins.
//! Context and service-tier overrides belong to PriceTier within the selected rule.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "price_rules")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub provider_id: Option<String>,
    pub model_pattern: String,
    /// None covers all operations of the matched model.
    pub operation: Option<String>,
    #[sea_orm(default_value = 0)]
    pub priority: i32,
    pub currency: String,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<Option<crate::entity::upstream::provider::Entity>>,
    #[sea_orm(has_many)]
    pub rates: HasMany<super::price_rate::Entity>,
    #[sea_orm(has_many)]
    pub tiers: HasMany<super::price_tier::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
