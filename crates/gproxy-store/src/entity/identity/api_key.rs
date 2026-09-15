//! Gateway API keys belonging to a user; credential secrets for upstreams are a separate entity.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "api_keys")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    pub name: String,
    #[sea_orm(unique)]
    pub key_hash: String,
    pub prefix: String,
    /// Optional retained secret for reveal/export; representation is defined by the key layer.
    pub secret: Option<Vec<u8>>,
    pub expires_at_ms: Option<i64>,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<super::user::Entity>,
    #[sea_orm(has_many)]
    pub permissions: HasMany<super::permission::Entity>,
    #[sea_orm(has_many)]
    pub rate_limits: HasMany<crate::entity::limits::rate_limit::Entity>,
    #[sea_orm(has_many)]
    pub quotas: HasMany<crate::entity::limits::quota::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
