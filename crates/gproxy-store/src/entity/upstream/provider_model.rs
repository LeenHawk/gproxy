//! Models exposed by a provider, optionally mapped to a global catalog entry.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "provider_models")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "upstream_model")]
    pub provider_id: String,
    #[sea_orm(unique_key = "upstream_model")]
    pub upstream_name: String,
    #[sea_orm(indexed)]
    pub model_id: Option<String>,
    pub metadata: Json,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<super::provider::Entity>,
    #[sea_orm(belongs_to, from = "model_id", to = "id", on_delete = "SetNull")]
    pub model: BelongsTo<Option<super::model::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
