//! Upstream credential acquisition. These traits do not make gproxy an OAuth
//! issuer. The host owns PKCE/state generation, callbacks and login persistence.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{OperationFuture, ProviderView};
use crate::OutboundClient;

#[derive(Clone, Copy)]
pub struct LoginContext<'a> {
    pub provider: ProviderView<'a>,
    pub client: &'a dyn OutboundClient,
}

/// Common token fields; provider-specific account/organization fields stay in
/// the provider envelope. Returned secrets are plaintext and are not Debug.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthCredential {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub token_type: Option<String>,
    pub scopes: Vec<String>,
    pub expires_at_ms: Option<i64>,
    pub refresh_expires_at_ms: Option<i64>,
    #[serde(default)]
    pub provider_fields: BTreeMap<String, Value>,
}

pub struct AuthorizationRequest<'a> {
    pub redirect_uri: &'a str,
    pub state: &'a str,
    /// S256 challenge; the verifier remains with the host until exchange.
    pub code_challenge: &'a str,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthorizationStart {
    pub authorize_url: String,
    pub redirect_uri: String,
}

pub struct AuthorizationCode<'a> {
    pub code: &'a str,
    pub redirect_uri: &'a str,
    pub code_verifier: &'a str,
    /// Original state, also required in Claude's upstream token exchange.
    pub state: &'a str,
}

pub trait OAuthAuthorizationCode: Send + Sync {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart>;

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential>;
}

/// Session passed back to poll. Some providers have device_auth_id rather than
/// a standard device_code; their implementation owns provider_state's shape.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_at_ms: Option<i64>,
    pub interval_secs: u64,
    pub provider_state: BTreeMap<String, Value>,
}

pub enum DevicePoll {
    Pending,
    /// The host uses this interval for the next poll; no internal sleep/loop.
    SlowDown {
        interval_secs: u64,
    },
    Ready(OAuthCredential),
    Denied,
    Expired,
}

pub trait OAuthDeviceCode: Send + Sync {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization>;

    /// One polling step; may include a final token exchange when authorization
    /// succeeds. Waiting and scheduling the next step belong to the caller.
    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll>;
}

/// ClaudeCode can acquire an OAuth credential from an existing browser cookie.
pub trait CookieLogin: Send + Sync {
    fn exchange_cookie<'a>(
        &'a self,
        context: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, OAuthCredential>;
}
