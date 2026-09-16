//! A global upstream configuration, shared across organizations, teams and users.
//! `channel` names an implementation in code; providers do not have membership owners.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "providers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    pub channel: String,
    #[sea_orm(column_type = "Text")]
    pub base_url: Option<String>,
    /// None inherits Setting.connection_profile_id; a credential may override it.
    #[sea_orm(indexed)]
    pub connection_profile_id: Option<String>,
    pub config: Json,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    pub created_at_ms: i64,
    #[sea_orm(
        belongs_to,
        from = "connection_profile_id",
        to = "id",
        on_delete = "Restrict"
    )]
    pub connection_profile: BelongsTo<Option<crate::entity::config::connection_profile::Entity>>,
    #[sea_orm(has_many)]
    pub credentials: HasMany<super::credential::Entity>,
    #[sea_orm(has_many)]
    pub models: HasMany<super::provider_model::Entity>,
    #[sea_orm(has_many)]
    pub route_members: HasMany<crate::entity::routing::route_member::Entity>,
    #[sea_orm(has_many)]
    pub operation_rules: HasMany<super::operation_rule::Entity>,
    #[sea_orm(has_many)]
    pub price_rules: HasMany<crate::entity::pricing::price_rule::Entity>,
    #[sea_orm(has_many)]
    pub permissions: HasMany<crate::entity::identity::permission::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
