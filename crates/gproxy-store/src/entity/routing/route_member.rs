//! One provider/upstream-model member of a route, matching current v3 execution.
//! Provider credential selection is separate; there is no fixed credential here.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "route_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub route_id: String,
    #[sea_orm(indexed)]
    pub provider_id: String,
    /// Explicit nonempty upstream model name; not a model-catalog foreign key.
    pub upstream_model: String,
    /// Lower tier is preferred; later tiers provide fallback candidates.
    #[sea_orm(default_value = 0)]
    pub tier: u32,
    /// Positive relative weight within a tier; also orders failover candidates.
    #[sea_orm(default_value = 100)]
    pub weight: u32,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "route_id", to = "id", on_delete = "Cascade")]
    pub route: BelongsTo<super::route::Entity>,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<crate::entity::upstream::provider::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
