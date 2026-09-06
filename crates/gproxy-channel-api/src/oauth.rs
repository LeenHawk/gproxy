use crate::{BoxFuture, CallerIdentity, MaybeSync};

pub const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const PI_OAUTH_CLIENT_ID: &str = "pi-gproxy";
pub const GPROXY_OAUTH_SCOPE: &str = "gproxy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClientInfo {
    pub client_id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthBrowserUser {
    pub identity: CallerIdentity,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAuthorizeInput {
    pub provider_id: Option<i64>,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub code_challenge: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCodeGrant {
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenSet {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthDeviceStart {
    pub device_auth_id: String,
    pub user_code: String,
    pub interval_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthDevicePoll {
    Pending,
    Ready {
        authorization_code: String,
        code_verifier: String,
        code_challenge: String,
    },
    Denied,
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("invalid request")]
    InvalidRequest,
    #[error("invalid client")]
    InvalidClient,
    #[error("invalid grant")]
    InvalidGrant,
    #[error("access denied")]
    AccessDenied,
    #[error("temporarily unavailable")]
    TemporarilyUnavailable,
    #[error("store: {0}")]
    Store(String),
}

pub trait OAuthService: MaybeSync {
    fn client<'a>(
        &'a self,
        client_id: &'a str,
    ) -> BoxFuture<'a, Result<OAuthClientInfo, OAuthError>>;

    fn browser_user<'a>(
        &'a self,
        headers: &'a http::HeaderMap,
    ) -> BoxFuture<'a, Result<Option<OAuthBrowserUser>, OAuthError>>;

    fn authorize<'a>(
        &'a self,
        user: &'a OAuthBrowserUser,
        input: OAuthAuthorizeInput,
    ) -> BoxFuture<'a, Result<OAuthCodeGrant, OAuthError>>;

    fn exchange_code<'a>(
        &'a self,
        code: &'a str,
        client_id: &'a str,
        redirect_uri: &'a str,
        verifier: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthTokenSet, OAuthError>>;

    fn refresh<'a>(
        &'a self,
        refresh_token: &'a str,
        client_id: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthTokenSet, OAuthError>>;

    fn revoke<'a>(&'a self, token: &'a str) -> BoxFuture<'a, Result<(), OAuthError>>;

    fn device_start<'a>(
        &'a self,
        provider_id: Option<i64>,
        client_id: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<OAuthDeviceStart, OAuthError>>;

    fn device_poll<'a>(
        &'a self,
        device_auth_id: &'a str,
        user_code: &'a str,
    ) -> BoxFuture<'a, Result<OAuthDevicePoll, OAuthError>>;

    fn device_approve<'a>(
        &'a self,
        user: &'a OAuthBrowserUser,
        user_code: &'a str,
        issuer: &'a str,
    ) -> BoxFuture<'a, Result<(), OAuthError>>;
}
