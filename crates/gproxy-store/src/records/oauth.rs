#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthGrantInput {
    pub user_id: i64,
    pub user_key_id: i64,
    pub provider_id: Option<i64>,
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
    pub provider_id: Option<i64>,
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
    pub scopes: String,
    pub client_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthDeviceInput {
    pub device_digest: Vec<u8>,
    pub user_code: String,
    pub client_id: String,
    pub provider_id: Option<i64>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthDeviceRecord {
    pub id: i64,
    pub user_code: String,
    pub client_id: String,
    pub provider_id: Option<i64>,
    pub expires_at: i64,
    pub grant_id: Option<i64>,
    pub approved_at: Option<i64>,
    pub consumed_at: Option<i64>,
    pub denied_at: Option<i64>,
    pub envelope: Option<super::CredentialEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClientInput {
    pub client_id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClientRecord {
    pub id: i64,
    pub client_id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub enabled: bool,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSessionRecord {
    pub id: i64,
    pub client_id: String,
    pub client_name: String,
    pub logged_in_at: i64,
    pub last_refreshed_at: Option<i64>,
    pub refresh_count: Option<i64>,
    pub refresh_expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSessionPage {
    pub sessions: Vec<OAuthSessionRecord>,
    pub total_logins: i64,
    pub active_sessions: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthExchangeSource {
    Code(i64),
    Refresh(i64),
}

#[derive(Debug, Clone)]
pub struct OAuthAuthorizationInput {
    pub key: super::UserKeyInput,
    pub provider_id: Option<i64>,
    pub client_id: String,
    pub scopes: String,
    pub chatgpt_user_id: String,
    pub chatgpt_account_id: String,
    pub code_digest: Vec<u8>,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub created_at: i64,
    pub expires_at: i64,
}
