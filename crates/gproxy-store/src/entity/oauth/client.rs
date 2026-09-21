//! Public OAuth clients. This registry does not issue client secrets.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_clients")]
pub struct Model {
    /// Immutable public OAuth client_id, used directly as the primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    /// JSON array of registered redirect URI strings, validated by the issuer.
    pub redirect_uris: Json,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    /// Soft deletion preserves session history; reactivation must not un-revoke grants.
    pub deleted_at_ms: Option<i64>,
    #[sea_orm(has_many)]
    pub grants: HasMany<super::grant::Entity>,
    #[sea_orm(has_many)]
    pub devices: HasMany<super::device::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
