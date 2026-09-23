//! Gateway API keys. The digest and the sealed secret never leave the crate;
//! the plaintext exists exactly twice, at the mint and at a rotation.

use super::double_option;
use gproxy_store::entity::identity::api_key::{self, ApiKeyKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ApiKeyDto {
    pub id: String,
    pub user_id: String,
    pub name: String,
    /// The first eight characters of the key body, for recognising a key in a
    /// list. Not a secret and not enough of one to be useful.
    pub prefix: String,
    /// `user` or `oauth`. An `oauth` key is a grant's internal key and is not
    /// a bearer credential; it is listed so an operator can see it, never
    /// created or revealed through this family.
    pub kind: String,
    pub organization_id: Option<String>,
    pub team_id: Option<String>,
    pub expires_at_ms: Option<i64>,
    pub enabled: bool,
    /// Whether the instance retained a sealed copy of the key text, i.e.
    /// whether `reveal` can answer. The bytes themselves have no field.
    pub has_secret: bool,
}

impl From<api_key::Model> for ApiKeyDto {
    fn from(row: api_key::Model) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            prefix: row.prefix,
            kind: kind_name(row.kind).to_owned(),
            organization_id: row.organization_id,
            team_id: row.team_id,
            expires_at_ms: row.expires_at_ms,
            enabled: row.enabled,
            has_secret: row.secret.is_some(),
        }
    }
}

pub(crate) fn kind_name(kind: ApiKeyKind) -> &'static str {
    match kind {
        ApiKeyKind::User => "user",
        ApiKeyKind::OAuth => "oauth",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ApiKeyWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub user_id: String,
    pub name: String,
    /// The binding that decides the budget chain, the permission subject and
    /// the credential-visibility boundary at once. It is set here, once, and
    /// never taken from a request header.
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Keep a sealed copy of the key text so it can be revealed later. False
    /// by default: a key the instance cannot reproduce is a key a database
    /// leak does not hand over.
    #[serde(default)]
    pub retain_secret: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ApiKeyPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub organization_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub team_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub expires_at_ms: Option<Option<i64>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// A minted or rotated key. `token` is the only time the plaintext exists in
/// a response; a caller that loses it rotates the key, it is not recoverable
/// unless the row was created with `retainSecret`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ApiKeyCreated {
    #[serde(flatten)]
    pub key: ApiKeyDto,
    pub token: String,
}

/// The answer to `reveal`, for a key whose secret the instance was asked to
/// retain. Separate from [`ApiKeyDto`] so a plaintext can never ride along
/// with an ordinary read.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ApiKeySecretDto {
    pub id: String,
    pub token: String,
}
