//! A credential references a global provider and is owned by an organization,
//! a team, or a user. Exactly one owner ID is set by credential management.
//! Organization/team credentials are usable by their descendant members, while
//! their contents and management operations are restricted to the owning scope's admins.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credentials")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub provider_id: String,
    /// Set for organization-owned credentials.
    #[sea_orm(indexed)]
    pub organization_id: Option<String>,
    /// Set for team-owned credentials; its organization is obtained through Team.
    #[sea_orm(indexed)]
    pub team_id: Option<String>,
    /// Set for personally owned credentials. This identifies the owner, not the creator.
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    pub label: Option<String>,
    pub auth_kind: String,
    /// Host-sealed secret payload. For OAuth, the common plaintext shape is
    /// OAuthCredentialSecret below; provider-specific data stays within its envelope.
    pub secret: Vec<u8>,
    /// Optimistic version for secret/config updates, including token refresh.
    /// Writers replace secret + expiry and increment version in one conditional write.
    #[sea_orm(default_value = 0)]
    pub version: i64,
    /// Credential proxy URL. None inherits Provider.proxy, then Setting.proxy.
    #[sea_orm(column_type = "Text")]
    pub proxy: Option<String>,
    pub metadata: Json,
    /// Access-token expiry for OAuth; updated atomically with secret/version.
    pub expires_at_ms: Option<i64>,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<super::provider::Entity>,
    #[sea_orm(belongs_to, from = "organization_id", to = "id", on_delete = "Cascade")]
    pub organization: BelongsTo<Option<crate::entity::identity::organization::Entity>>,
    #[sea_orm(belongs_to, from = "team_id", to = "id", on_delete = "Cascade")]
    pub team: BelongsTo<Option<crate::entity::identity::team::Entity>>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<Option<crate::entity::identity::user::Entity>>,
    #[sea_orm(has_one)]
    pub subscription_pool_member: HasOne<crate::entity::subscription::pool_member::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

/// Common plaintext inside an upstream OAuth credential's sealed secret blob.
/// Kept out of Debug output; this is a data contract, not an encryption codec.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthCredentialSecret {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub token_type: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub refresh_expires_at_ms: Option<i64>,
    /// Provider-specific secret fields, not public credential metadata.
    #[serde(default)]
    pub provider_fields: std::collections::BTreeMap<String, Json>,
}
