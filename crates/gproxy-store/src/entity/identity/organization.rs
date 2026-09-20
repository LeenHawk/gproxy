//! Organizations own shared credentials and contain teams.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organizations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    /// OAuth client-ID string array. None inherits; [] denies this scope.
    /// Configured organizations are unioned, then intersected with other levels.
    pub oauth_client_allowlist: Option<Json>,
    pub created_at_ms: i64,
    #[sea_orm(has_many)]
    pub teams: HasMany<super::team::Entity>,
    #[sea_orm(has_many)]
    pub members: HasMany<super::organization_member::Entity>,
    #[sea_orm(has_many)]
    pub credentials: HasMany<crate::entity::upstream::credential::Entity>,
    /// Gateway keys bound to this scope; see ApiKey for what the binding decides.
    #[sea_orm(has_many)]
    pub api_keys: HasMany<super::api_key::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
