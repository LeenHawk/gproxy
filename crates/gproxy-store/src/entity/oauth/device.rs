//! Pending/approved/denied/consumed device authorizations for downstream clients.
//! A grant is attached on approval. Client, scopes and grant must agree.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// SHA-256 digest of the secret device code.
    #[sea_orm(unique, column_type = "Binary(32)")]
    pub device_code_hash: Vec<u8>,
    /// Short normalized code entered by the user; distinct from secret device_code.
    #[sea_orm(unique)]
    pub user_code: String,
    #[sea_orm(indexed)]
    pub client_id: String,
    /// JSON array of requested OAuth scope strings.
    pub scopes: Json,
    #[sea_orm(indexed)]
    pub grant_id: Option<String>,
    pub created_at_ms: i64,
    #[sea_orm(indexed)]
    pub expires_at_ms: i64,
    pub approved_at_ms: Option<i64>,
    pub denied_at_ms: Option<i64>,
    pub consumed_at_ms: Option<i64>,
    /// Internal receipt guarding atomic device approval + grant/code creation.
    #[sea_orm(column_type = "Binary(32)")]
    pub approval_receipt: Option<Vec<u8>>,
    /// Host-sealed envelope with authorization_code/code_verifier/code_challenge
    /// for the legacy Codex device adapter. Generic device grants need no payload.
    pub authorization_payload: Option<Vec<u8>>,
    #[sea_orm(belongs_to, from = "client_id", to = "id", on_delete = "Cascade")]
    pub client: BelongsTo<super::client::Entity>,
    #[sea_orm(belongs_to, from = "grant_id", to = "id", on_delete = "Cascade")]
    pub grant: BelongsTo<Option<super::grant::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
