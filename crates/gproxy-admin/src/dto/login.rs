use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::IdResponse;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AuthCodeStartRequest {
    pub provider_id: i64,
    #[serde(default)]
    #[ts(type = "unknown | null")]
    pub params: Option<Value>,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuthCodeStartResponse {
    pub login_session_id: String,
    pub authorize_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuthCodeCompleteRequest {
    pub login_session_id: String,
    pub callback_url: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeviceStartRequest {
    pub provider_id: i64,
    #[serde(default)]
    #[ts(type = "unknown | null")]
    pub params: Option<Value>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeviceStartResponse {
    pub login_session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DevicePollRequest {
    pub login_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(tag = "status", rename_all = "snake_case")]
pub enum DevicePollResponse {
    Pending,
    Ready { credential: IdResponse },
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CookieExchangeRequest {
    pub provider_id: i64,
    pub cookie: String,
    pub label: Option<String>,
}
