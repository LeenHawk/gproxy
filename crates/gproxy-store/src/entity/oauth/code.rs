//! One-time authorization code, bound to a grant, redirect URI and PKCE S256.
//! Store only its hash; state is echoed by the authorize endpoint, not a token.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_codes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// SHA-256 digest, with a fixed-size binary column for portable unique indexes.
    #[sea_orm(unique, column_type = "Binary(32)")]
    pub code_hash: Vec<u8>,
    #[sea_orm(indexed)]
    pub grant_id: String,
    #[sea_orm(column_type = "Text")]
    pub redirect_uri: String,
    /// Base64url SHA-256 challenge. This issuer supports S256 only.
    pub code_challenge: String,
    pub created_at_ms: i64,
    #[sea_orm(indexed)]
    pub expires_at_ms: i64,
    pub consumed_at_ms: Option<i64>,
    /// Fresh per-attempt receipt tying consumption to dependent writes. It is
    /// not reused when retrying an exchange after an uncertain response.
    #[sea_orm(column_type = "Binary(32)")]
    pub consumed_by: Option<Vec<u8>>,
    #[sea_orm(belongs_to, from = "grant_id", to = "id", on_delete = "Cascade")]
    pub grant: BelongsTo<super::grant::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
