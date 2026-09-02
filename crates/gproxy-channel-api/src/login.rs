//! First-time credential acquisition, separate from credential refresh.

use serde_json::Value;

use crate::{BoxFuture, ChannelError, SimpleHttp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMode {
    AuthCode,
    Device,
    Cookie,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginParamKind {
    Text,
    Select,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    ApiKey,
    Oauth,
    Cookie,
}

impl CredentialKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Oauth => "oauth",
            Self::Cookie => "cookie",
        }
    }
}

#[derive(Debug)]
pub struct CredentialAcquisition {
    pub kind: CredentialKind,
    pub secret: Value,
}

impl CredentialAcquisition {
    pub const fn new(kind: CredentialKind, secret: Value) -> Self {
        Self { kind, secret }
    }

    pub const fn oauth(secret: Value) -> Self {
        Self::new(CredentialKind::Oauth, secret)
    }

    pub const fn api_key(secret: Value) -> Self {
        Self::new(CredentialKind::ApiKey, secret)
    }

    pub const fn cookie(secret: Value) -> Self {
        Self::new(CredentialKind::Cookie, secret)
    }
}

#[derive(Debug)]
pub struct LoginParamCondition {
    pub param: &'static str,
    pub equals: &'static str,
}

#[derive(Debug)]
pub struct LoginParam {
    pub name: &'static str,
    pub kind: LoginParamKind,
    pub required: bool,
    pub default_value: Option<&'static str>,
    pub options: &'static [&'static str],
    pub modes: &'static [LoginMode],
    pub condition: Option<LoginParamCondition>,
}

#[derive(Debug)]
pub struct LoginDescriptor {
    pub modes: &'static [LoginMode],
    pub params: &'static [LoginParam],
}

pub struct ChannelLoginRef<'a> {
    pub adapter: &'a dyn ChannelLogin,
    pub descriptor: &'static LoginDescriptor,
}

pub struct AuthCodeStart {
    pub authorize_url: String,
    pub redirect_uri: String,
    pub extra: Option<Value>,
}

pub struct DeviceInit {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval_secs: u64,
}

#[derive(Debug)]
pub enum DevicePoll {
    Pending,
    Ready(CredentialAcquisition),
    Denied,
}

pub struct AuthCodeStartCtx<'a> {
    pub provider_settings: &'a Value,
    pub params: &'a Value,
    pub redirect_uri: &'a str,
    pub state: &'a str,
    pub pkce_challenge: &'a str,
}

pub struct AuthCodeExchangeCtx<'a> {
    pub provider_settings: &'a Value,
    pub code: &'a str,
    pub verifier: &'a str,
    pub redirect_uri: &'a str,
    pub extra: Option<&'a Value>,
}

pub struct DeviceStartCtx<'a> {
    pub provider_settings: &'a Value,
    pub params: &'a Value,
}

pub struct DevicePollCtx<'a> {
    pub provider_settings: &'a Value,
    pub device_code: &'a str,
}

pub struct CookieExchangeCtx<'a> {
    pub provider_settings: &'a Value,
    pub cookie: &'a str,
}

/// Pure provider login. Returned secrets are plaintext; callers seal and store.
pub trait ChannelLogin: Send + Sync {
    fn authcode_start<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        _ctx: AuthCodeStartCtx<'a>,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, ChannelError>> {
        Box::pin(async { Ok(None) })
    }

    fn authcode_exchange<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        _ctx: AuthCodeExchangeCtx<'a>,
    ) -> BoxFuture<'a, Result<CredentialAcquisition, ChannelError>> {
        Box::pin(async { Err(ChannelError::Unsupported("authcode login")) })
    }

    fn device_start<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        _ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async { Err(ChannelError::Unsupported("device login")) })
    }

    fn device_poll<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        _ctx: DevicePollCtx<'a>,
    ) -> BoxFuture<'a, Result<DevicePoll, ChannelError>> {
        Box::pin(async { Err(ChannelError::Unsupported("device login")) })
    }

    fn cookie_exchange<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        _ctx: CookieExchangeCtx<'a>,
    ) -> BoxFuture<'a, Result<CredentialAcquisition, ChannelError>> {
        Box::pin(async { Err(ChannelError::Unsupported("cookie login")) })
    }
}
