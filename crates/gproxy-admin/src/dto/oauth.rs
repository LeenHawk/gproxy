use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthClientDto {
    pub id: i64,
    pub client_id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OAuthClientWriteRequest {
    pub client_id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthSessionDto {
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthSessionPageDto {
    pub sessions: Vec<OAuthSessionDto>,
    pub total_logins: i64,
    pub active_sessions: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthAuthorizationRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthConsentDto {
    pub client_id: String,
    pub client_name: String,
    pub user_name: String,
    pub scope: String,
    pub user_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthAuthorizeDecision {
    pub authorization: OAuthAuthorizationRequest,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthRedirectDto {
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthDeviceDecision {
    pub user_code: String,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OAuthErrorDto {
    pub error: String,
}
