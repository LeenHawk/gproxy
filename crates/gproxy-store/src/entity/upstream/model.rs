//! Global model metadata. Routing can also match names absent from this catalog.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "models")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    pub metadata: Json,
    pub vocabulary_file_id: Option<String>,
    #[sea_orm(
        belongs_to,
        from = "vocabulary_file_id",
        to = "id",
        on_delete = "SetNull"
    )]
    pub vocabulary_file: BelongsTo<Option<crate::entity::resource::file_object::Entity>>,
    #[sea_orm(has_many)]
    pub providers: HasMany<super::provider_model::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
