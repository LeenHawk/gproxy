//! Match client model names and choose among configured route targets.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "routes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    /// None means the global route scope.
    #[sea_orm(indexed)]
    pub namespace_id: Option<String>,
    pub model_pattern: String,
    pub strategy: String,
    #[sea_orm(default_value = 0)]
    pub priority: i32,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "namespace_id", to = "id", on_delete = "Cascade")]
    pub namespace: BelongsTo<Option<super::namespace::Entity>>,
    #[sea_orm(has_many)]
    pub targets: HasMany<super::route_target::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
