use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use dashmap::DashMap;
use rand::Rng;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::{Value, json};
use sha2::Digest;
use url::form_urlencoded;
use wreq::{Client as WreqClient, Method as WreqMethod};

use super::constants::{
    CLIENT_ID, DEFAULT_BROWSER_REDIRECT_URI, DEFAULT_ISSUER, OAUTH_ORIGINATOR, OAUTH_SCOPE,
    OAUTH_STATE_TTL_MS, TOKEN_REFRESH_SKEW_MS,
};
use super::credential::CodexCredential;
use crate::channels::ChannelSettings;
use crate::channels::upstream::{
    UpstreamError, UpstreamOAuthCallbackResult, UpstreamOAuthCredential, UpstreamOAuthRequest,
    UpstreamOAuthResponse, UpstreamRequestMeta, tracked_request, tracked_send_request,
};
use crate::channels::{BuiltinChannelCredential, ChannelCredential};

#[derive(Debug, Clone)]
enum OAuthMode {
    DeviceAuth,
    AuthorizationCode,
}

#[derive(Debug, Clone)]
enum OAuthState {
    DeviceAuth {
        device_auth_id: String,
        user_code: String,
        interval_secs: u64,
        created_at_unix_ms: u64,
    },
    AuthorizationCode {
        code_verifier: String,
        redirect_uri: String,
        created_at_unix_ms: u64,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct CodexAuthMaterial {
    access_token: String,
    refresh_token: String,
    id_token: String,
    pub(crate) account_id: String,
    pub(crate) priority_tier: Option<bool>,
    expires_at_unix_ms: u64,
}

impl CodexAuthMaterial {
    fn access_token_valid(&self, now_unix_ms: u64) -> bool {
        !self.access_token.trim().is_empty()
            && self
                .expires_at_unix_ms
                .saturating_sub(TOKEN_REFRESH_SKEW_MS)
                > now_unix_ms
    }

    fn can_refresh(&self) -> bool {
        !self.refresh_token.trim().is_empty()
    }
}

#[derive(Debug, Clone)]
struct CachedCodexToken {
    access_token: String,
    expires_at_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct CodexRefreshedToken {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_at_unix_ms: u64,
    pub(crate) user_email: Option<String>,
    pub(crate) id_token: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CodexResolvedAccessToken {
    pub(crate) access_token: String,
    pub(crate) refreshed: Option<CodexRefreshedToken>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CodexTokenRefreshError {
    #[error("invalid codex credential: {0}")]
    InvalidCredential(String),
    #[error("transient codex token refresh error: {0}")]
    Transient(String),
}

impl CodexTokenRefreshError {
    pub(crate) fn as_message(&self) -> String {
        self.to_string()
    }

    pub(crate) fn is_invalid_credential(&self) -> bool {
        matches!(self, Self::InvalidCredential(_))
    }
}

#[derive(Debug, Deserialize)]
struct DeviceUserCodeResponse {
    device_auth_id: String,
    #[serde(alias = "user_code", alias = "usercode")]
    user_code: String,
    #[serde(
        default = "default_poll_interval_secs",
        deserialize_with = "deserialize_u64_from_string"
    )]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenPollResponse {
    authorization_code: String,
    code_verifier: String,
}

#[derive(Debug)]
enum DeviceAuthPollStatus {
    Pending,
    Authorized(DeviceTokenPollResponse),
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_u64_from_string")]
    expires_in: Option<u64>,
    error: Option<Value>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum U64StringOrNumber {
    String(String),
    Number(u64),
}

fn deserialize_u64_from_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = U64StringOrNumber::deserialize(deserializer)?;
    match value {
        U64StringOrNumber::String(value) => value
            .trim()
            .parse::<u64>()
            .map_err(|err| de::Error::custom(format!("invalid u64 string: {err}"))),
        U64StringOrNumber::Number(value) => Ok(value),
    }
}

fn deserialize_option_u64_from_string<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<U64StringOrNumber>::deserialize(deserializer)?;
    value
        .map(|item| match item {
            U64StringOrNumber::String(value) => value
                .trim()
                .parse::<u64>()
                .map_err(|err| de::Error::custom(format!("invalid u64 string: {err}"))),
            U64StringOrNumber::Number(value) => Ok(value),
        })
        .transpose()
}

#[derive(Debug, Default)]
struct IdTokenClaims {
    email: Option<String>,
    plan: Option<String>,
    account_id: Option<String>,
}

fn oauth_states() -> &'static DashMap<String, OAuthState> {
    static STATES: OnceLock<DashMap<String, OAuthState>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn codex_token_cache() -> &'static DashMap<String, CachedCodexToken> {
    static CACHE: OnceLock<DashMap<String, CachedCodexToken>> = OnceLock::new();
    CACHE.get_or_init(DashMap::new)
}

mod entry;
pub use entry::{execute_codex_oauth_callback, execute_codex_oauth_start};
mod flow;
use flow::*;
mod token;
use token::*;
pub(crate) use token::{codex_auth_material_from_credential, resolve_codex_access_token};

#[cfg(test)]
mod tests;
