//! Gateway API keys belonging to a user; credential secrets for upstreams are a separate entity.
//!
//! A key is bound to at most one organization and at most one team. That single
//! binding decides three things at once in the application layer: the budget
//! owner chain the call settles against, the subject scope permission rules are
//! evaluated for, and the credential-visibility boundary the key may select
//! from. It is deliberately not taken from a request header, so a client cannot
//! choose who is billed or what it can see. Those are application-level
//! semantics; this table only holds the columns.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "api_keys")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    /// Set when the key is bound to an organization.
    #[sea_orm(indexed)]
    pub organization_id: Option<String>,
    /// Set when the key is bound to a team; its organization is obtained through Team.
    #[sea_orm(indexed)]
    pub team_id: Option<String>,
    pub name: String,
    /// OAuth keys carry a grant's policy/usage identity and are not ordinary
    /// bearer API keys. The auth/admin layer must enforce that separation.
    #[sea_orm(default_value = "user")]
    pub kind: ApiKeyKind,
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
    /// Deleting the organization deletes its bound keys: a key whose owner chain
    /// and visibility boundary no longer exist must not keep serving requests.
    #[sea_orm(belongs_to, from = "organization_id", to = "id", on_delete = "Cascade")]
    pub organization: BelongsTo<Option<super::organization::Entity>>,
    /// Deleting the team deletes its bound keys, for the same reason.
    #[sea_orm(belongs_to, from = "team_id", to = "id", on_delete = "Cascade")]
    pub team: BelongsTo<Option<super::team::Entity>>,
    #[sea_orm(has_many)]
    pub permissions: HasMany<super::permission::Entity>,
    #[sea_orm(has_many)]
    pub rate_limits: HasMany<crate::entity::limits::rate_limit::Entity>,
    #[sea_orm(has_one)]
    pub oauth_grant: HasOne<crate::entity::oauth::grant::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ApiKeyKind {
    #[sea_orm(string_value = "user")]
    User,
    #[sea_orm(string_value = "oauth")]
    OAuth,
}
