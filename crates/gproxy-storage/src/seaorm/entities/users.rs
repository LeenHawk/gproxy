use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    #[sea_orm(unique_key = "user_name")]
    pub name: String,
    pub password: Option<String>,
    pub enabled: bool,
    pub is_admin: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    #[sea_orm(has_many)]
    pub keys: HasMany<super::user_keys::Entity>,
    #[sea_orm(has_many)]
    pub downstream_requests: HasMany<super::downstream_requests::Entity>,
    #[sea_orm(has_many)]
    pub usages: HasMany<super::usages::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
