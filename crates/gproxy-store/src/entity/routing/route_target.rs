//! A route candidate selects a provider; the runtime credential pool selects a credential.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "route_targets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub route_id: String,
    #[sea_orm(indexed)]
    pub provider_id: String,
    /// None forwards the requested model name. This is not a model-catalog foreign key.
    pub upstream_model: Option<String>,
    #[sea_orm(default_value = 1)]
    pub weight: u32,
    #[sea_orm(default_value = 0)]
    pub priority: i32,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "route_id", to = "id", on_delete = "Cascade")]
    pub route: BelongsTo<super::route::Entity>,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<crate::entity::upstream::provider::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
