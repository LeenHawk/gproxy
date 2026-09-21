//! Issued access/refresh token hashes, never the plaintext bearer value.
//! ID tokens are signed response claims rather than a third refreshable token kind.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// SHA-256 digest.
    #[sea_orm(unique, column_type = "Binary(32)")]
    pub token_hash: Vec<u8>,
    #[sea_orm(indexed)]
    pub grant_id: String,
    pub kind: TokenKind,
    pub created_at_ms: i64,
    #[sea_orm(indexed)]
    pub expires_at_ms: i64,
    /// Refresh-token consumption; access tokens are not single-use.
    pub consumed_at_ms: Option<i64>,
    /// Fresh per-attempt receipt, written atomically with consumption.
    #[sea_orm(column_type = "Binary(32)")]
    pub consumed_by: Option<Vec<u8>>,
    pub revoked_at_ms: Option<i64>,
    #[sea_orm(belongs_to, from = "grant_id", to = "id", on_delete = "Cascade")]
    pub grant: BelongsTo<super::grant::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum TokenKind {
    #[sea_orm(string_value = "access")]
    Access,
    #[sea_orm(string_value = "refresh")]
    Refresh,
}
