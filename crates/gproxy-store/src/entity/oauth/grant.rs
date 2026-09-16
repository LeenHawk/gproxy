//! One downstream authorization/session, across all eligible upstream providers.
//! Revocation preserves history and invalidates the entire token family. No
//! provider/credential ownership: upstream accounts are chosen through routing.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_grants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    /// One internal OAuth API key per grant, owned by user_id. The writer checks
    /// key kind/owner; OAuth keys cannot authenticate as ordinary API keys.
    #[sea_orm(unique)]
    pub api_key_id: String,
    #[sea_orm(indexed)]
    pub client_id: String,
    /// JSON array of granted OAuth scope strings.
    pub scopes: Json,
    /// Stable gateway subject emitted in compatibility ID-token claims.
    pub subject: String,
    /// Optional gateway account identity for Codex-compatible claims, not an
    /// upstream account ID and not an organization/team permission assignment.
    pub account_id: Option<String>,
    pub created_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
    /// Initial successful token exchange; consent alone is not a completed login.
    pub logged_in_at_ms: Option<i64>,
    pub last_refreshed_at_ms: Option<i64>,
    #[sea_orm(default_value = 0)]
    pub refresh_count: u64,
    pub refresh_expires_at_ms: Option<i64>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<crate::entity::identity::user::Entity>,
    #[sea_orm(belongs_to, from = "api_key_id", to = "id", on_delete = "Cascade")]
    pub api_key: BelongsTo<crate::entity::identity::api_key::Entity>,
    #[sea_orm(belongs_to, from = "client_id", to = "id", on_delete = "Cascade")]
    pub client: BelongsTo<super::client::Entity>,
    #[sea_orm(has_many)]
    pub codes: HasMany<super::code::Entity>,
    #[sea_orm(has_many)]
    pub tokens: HasMany<super::token::Entity>,
    #[sea_orm(has_many)]
    pub devices: HasMany<super::device::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
