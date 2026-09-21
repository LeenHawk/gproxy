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
    /// Public account facts. They are sealed with the tokens *and* published
    /// as the credential's `metadata`, which a console may render, so nothing
    /// secret belongs here.
    #[serde(default)]
    pub provider_fields: BTreeMap<String, Value>,
    /// Secret-bearing facts a later refresh needs: sealed with the tokens and
    /// never published as metadata. A client secret a login registered for
    /// itself is the case this exists for — `provider_fields` would leak it,
    /// and the login session it arrived in does not outlive the login.
    #[serde(default)]
    pub provider_secrets: BTreeMap<String, Value>,
}

pub struct AuthorizationRequest<'a> {
    pub redirect_uri: &'a str,
    pub state: &'a str,
    /// S256 challenge; the verifier remains with the host until exchange.
    pub code_challenge: &'a str,
}

/// What the browser step settled on. `provider_state` is the same pocket the
/// device flow carries in [`DeviceAuthorization`], so the two flows are one
/// idea: a channel may need facts of its own on the far side of the person's
/// authorization, and this is where they wait.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthorizationStart {
    pub authorize_url: String,
    pub redirect_uri: String,
    /// Facts this channel needs across the user's authorization, handed back
    /// to `exchange` in [`AuthorizationCode::provider_state`]. The shape is
    /// the channel's own.
    ///
    /// Unlike [`OAuthCredential::provider_fields`] this map **may carry
    /// secrets** — a client id and secret a dynamic registration (RFC 7591)
    /// just minted is what it exists for. It never leaves the host's login
    /// session, which is short-lived, cache-backed and never rendered, and it
    /// is never persisted on the credential. A channel that wants one of
    /// these facts to survive the login has to return it from `exchange`:
    /// public ones in `provider_fields`, secret ones in
    /// [`OAuthCredential::provider_secrets`].
    pub provider_state: BTreeMap<String, Value>,
}

pub struct AuthorizationCode<'a> {
    pub code: &'a str,
    pub redirect_uri: &'a str,
    pub code_verifier: &'a str,
    /// Original state, also required in Claude's upstream token exchange.
    pub state: &'a str,
    /// Whatever `authorize` put in [`AuthorizationStart::provider_state`],
    /// parked in the login session in between and handed back unchanged;
    /// empty when the channel needed nothing. Secret-bearing by design, so it
    /// may be sent to the upstream but never published: see the field it came
    /// from.
    pub provider_state: &'a BTreeMap<String, Value>,
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
    /// Facts this channel needs across the user's authorization, handed back
    /// to `poll` with the rest of the authorization. The same pocket, with the
    /// same rules, as [`AuthorizationStart::provider_state`]: the shape is the
    /// channel's own, it **may carry secrets** — unlike
    /// [`OAuthCredential::provider_fields`] — and it never leaves the host's
    /// login session.
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

/// A credential a login produced, in the channel's own secret shape: OAuth
/// tokens for ClaudeCode, a session cookie plus organization for Claude Web.
/// `metadata` holds public facts the host persists on the credential row
/// (plan, account id, model list), readable later through `CredentialView`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AcquiredCredential {
    pub secret: Value,
    pub expires_at_ms: Option<i64>,
    #[serde(default)]
    pub metadata: Value,
}

impl From<OAuthCredential> for AcquiredCredential {
    /// The whole credential is sealed; only `provider_fields` is published.
    /// `provider_secrets` is deliberately absent from the metadata: it is in
    /// the sealed blob beside the tokens and nowhere else.
    fn from(credential: OAuthCredential) -> Self {
        let expires_at_ms = credential.expires_at_ms;
        let metadata = Value::Object(
            credential
                .provider_fields
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        Self {
            secret: serde_json::to_value(&credential).unwrap_or(Value::Null),
            expires_at_ms,
            metadata,
        }
    }
}

/// ClaudeCode and Claude Web acquire a credential from an existing browser
/// cookie; what comes back is the channel's own secret shape.
pub trait CookieLogin: Send + Sync {
    fn exchange_cookie<'a>(
        &'a self,
        context: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, AcquiredCredential>;
}
