#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthGrantInput {
    pub user_id: i64,
    pub user_key_id: i64,
    pub provider_id: i64,
    pub client_id: String,
    pub scopes: String,
    pub chatgpt_user_id: String,
    pub chatgpt_account_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthGrantRecord {
    pub id: i64,
    pub user_id: i64,
    pub user_key_id: i64,
    pub provider_id: i64,
    pub client_id: String,
    pub scopes: String,
    pub chatgpt_user_id: String,
    pub chatgpt_account_id: String,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCodeInput {
    pub digest: Vec<u8>,
    pub grant_id: i64,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCodeRecord {
    pub id: i64,
    pub grant: OAuthGrantRecord,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenInput {
    pub digest: Vec<u8>,
    pub grant_id: i64,
    pub kind: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenRecord {
    pub id: i64,
    pub grant: OAuthGrantRecord,
    pub kind: String,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAccessIdentity {
    pub user_id: i64,
    pub user_key_id: i64,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthDeviceInput {
    pub device_digest: Vec<u8>,
    pub user_code: String,
    pub client_id: String,
    pub provider_id: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthDeviceRecord {
    pub id: i64,
    pub user_code: String,
    pub client_id: String,
    pub provider_id: i64,
    pub expires_at: i64,
    pub grant_id: Option<i64>,
    pub approved_at: Option<i64>,
    pub consumed_at: Option<i64>,
    pub envelope: Option<super::CredentialEnvelope>,
}
