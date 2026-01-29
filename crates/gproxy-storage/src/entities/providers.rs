use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "providers")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique_key = "provider_name")]
    pub name: String,
    pub config_json: Json,
    pub enabled: bool,
    pub updated_at: OffsetDateTime,
    #[sea_orm(has_many)]
    pub credentials: HasMany<super::credentials::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
