//! Configured rate limits. Live counters belong to the cache layer. Owner binding is the same draft as permissions.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "rate_limits")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    pub metric: String,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub limit_value: Decimal,
    pub period_seconds: i64,
    pub model_pattern: Option<String>,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<Option<crate::entity::identity::user::Entity>>,
    #[sea_orm(belongs_to, from = "api_key_id", to = "id", on_delete = "Cascade")]
    pub api_key: BelongsTo<Option<crate::entity::identity::api_key::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
