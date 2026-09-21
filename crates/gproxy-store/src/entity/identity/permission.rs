//! Draft ownership: a rule targets a user or an API key. Review this binding shape before implementing policy CRUD.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "permissions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    #[sea_orm(indexed)]
    pub provider_id: Option<String>,
    pub model_pattern: String,
    pub operation: Option<String>,
    pub action: String,
    #[sea_orm(default_value = 0)]
    pub priority: i32,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<Option<super::user::Entity>>,
    #[sea_orm(belongs_to, from = "api_key_id", to = "id", on_delete = "Cascade")]
    pub api_key: BelongsTo<Option<super::api_key::Entity>>,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<Option<crate::entity::upstream::provider::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
