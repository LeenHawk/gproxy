//! Gateway users. Organization and team roles are held in membership records.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub password_hash: Option<String>,
    /// Instance-wide role. Organization/team administrator roles are separate.
    pub role: String,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    /// OAuth client-ID string array. None inherits; [] denies all for this user.
    /// Intersected with global, organization and team allowlists.
    pub oauth_client_allowlist: Option<Json>,
    pub created_at_ms: i64,
    #[sea_orm(has_many)]
    pub organization_memberships: HasMany<super::organization_member::Entity>,
    #[sea_orm(has_many)]
    pub team_memberships: HasMany<super::team_member::Entity>,
    #[sea_orm(has_many)]
    pub credentials: HasMany<crate::entity::upstream::credential::Entity>,
    #[sea_orm(has_many)]
    pub api_keys: HasMany<super::api_key::Entity>,
    #[sea_orm(has_many)]
    pub sessions: HasMany<super::user_session::Entity>,
    #[sea_orm(has_many)]
    pub oauth_grants: HasMany<crate::entity::oauth::grant::Entity>,
    #[sea_orm(has_many)]
    pub permissions: HasMany<super::permission::Entity>,
    #[sea_orm(has_many)]
    pub rate_limits: HasMany<crate::entity::limits::rate_limit::Entity>,
    #[sea_orm(has_many)]
    pub resource_bindings: HasMany<crate::entity::resource::resource_binding::Entity>,
    #[sea_orm(has_many)]
    pub agent_sessions: HasMany<crate::entity::resource::agent_session::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
