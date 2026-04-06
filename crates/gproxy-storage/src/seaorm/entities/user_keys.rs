use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_keys")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    pub user_id: i64,
    pub api_key_ciphertext: String,
    #[sea_orm(unique_key = "user_api_key_digest")]
    pub api_key_digest: String,
    pub label: Option<String>,
    pub enabled: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: HasOne<super::users::Entity>,
    #[sea_orm(has_many)]
    pub downstream_requests: HasMany<super::downstream_requests::Entity>,
    #[sea_orm(has_many)]
    pub usages: HasMany<super::usages::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
