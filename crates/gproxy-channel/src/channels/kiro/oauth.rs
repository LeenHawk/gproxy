//! How a Kiro credential comes into existence and how it stays alive.
//!
//! Two products issue one, and they are different upstreams (v3
//! `kiro/{login,auth}.rs`):
//!
//! * The **Kiro desktop app** runs its own device flow at
//!   `prod.us-east-1.auth.desktop.kiro.dev`: `POST /oauth/device/authorization`
//!   starts it with `{clientId: "Kiro-CLI", loginProvider}`, `POST
//!   /oauth/device/poll` returns `{status}` until `authorized`, and `POST
//!   /refreshToken` renews with `{refreshToken}`. No client secret exists.
//! * **IAM Identity Center** issues one through AWS OIDC at
//!   `oidc.{region}.amazonaws.com`: the browser step is `/authorize` with
//!   PKCE, the exchange and the refresh are `/token` with a registered
//!   client id and secret.
//!
//! v3 registered that OIDC client itself, during `authcode_start`, and carried
//! the resulting id and secret to the exchange in the login session's `extra`
//! field. v4 does the same through `AuthorizationStart::provider_state`, the
//! pocket both login flows carry: `authorize` calls `RegisterClient` (RFC 7591)
//! and puts the pair there, and `exchange` reads it back. The public id goes
//! into `provider_fields` for credential metadata; the secret is sealed with the
//! tokens in `provider_secrets` because the refresh needs it and the login
//! session it arrived in is gone by then.
//!
//! An Identity Center that has dynamic registration disabled refuses that call.
//! Identity Center clients are registered during login and stored with that
//! credential. Imported credentials may carry the same pair in v3's flat
//! `client_id` / `client_secret` fields.

use super::config::{DEFAULT_REDIRECT_URI, ID, KiroConfig, OAUTH_SCOPE, validate_region};
use super::{fact, form_encode, json_headers, non_empty, send, unix_now_ms};
use crate::OutboundClient;
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, CredentialView, DeviceAuthorization, DevicePoll, LoginContext,
    OAuthAuthorizationCode, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use http::{HeaderMap, Method, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The client name both the device authorization and the OIDC registration
/// use (v3 `login.rs`).
const CLIENT_NAME: &str = "Kiro-CLI";
/// The registered client's id, under the same name wherever it travels: the
/// login session's `provider_state`, then the credential's `provider_fields`.
const CLIENT_ID: &str = "client_id";
/// Its secret, in `provider_state` and then in `provider_secrets` — never in
/// `provider_fields`, which the host publishes as metadata.
const CLIENT_SECRET: &str = "client_secret";
/// When that secret stops working, in unix milliseconds: a public fact that
/// travels beside the id. The Q Developer CLI drops a registration past this
/// stamp and stops refreshing (`builder_id.rs` `load_from_secret_store`),
/// since no refresh can renew the client itself.
const CLIENT_SECRET_EXPIRES_AT_MS: &str = "client_secret_expires_at_ms";
/// The same CLI's margin for a stamp that might pass in transit.
const EXPIRY_MARGIN_MS: i64 = 60_000;
/// What the upstream grants when it does not say.
const DEFAULT_EXPIRES_IN_SECS: i64 = 3_600;

fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("{ID}: {}", message.into()))
}

fn oidc_base(region: &str) -> Result<String, ChannelError> {
    validate_region(region)?;
    Ok(format!("https://oidc.{region}.amazonaws.com"))
}

fn expires_at_ms(seconds: Option<i64>) -> i64 {
    unix_now_ms().saturating_add(
        seconds
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_EXPIRES_IN_SECS)
            .saturating_mul(1_000),
    )
}

fn required(value: Option<String>, name: &str) -> Result<String, ChannelError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid_response(format!("the reply has no {name}")))
}

fn field(fields: &mut BTreeMap<String, Value>, name: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        fields.insert(name.into(), Value::String(value));
    }
}

fn credential(
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    provider_fields: BTreeMap<String, Value>,
    provider_secrets: BTreeMap<String, Value>,
) -> OAuthCredential {
    OAuthCredential {
        access_token,
        refresh_token,
        id_token: None,
        token_type: Some("Bearer".into()),
        scopes: OAUTH_SCOPE.split(' ').map(str::to_owned).collect(),
        expires_at_ms: Some(expires_at_ms(expires_in)),
        refresh_expires_at_ms: None,
        provider_fields,
        provider_secrets,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    #[serde(default)]
    verification_uri: Option<String>,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    /// The desktop service states its timings in milliseconds (KiroCrew
    /// `login/device.py`: `expiresInMilliseconds`, `intervalInMilliseconds`);
    /// the second-based names are read when those are absent.
    #[serde(default)]
    interval_in_milliseconds: Option<u64>,
    #[serde(default)]
    interval: Option<u64>,
    #[serde(default)]
    expires_in_milliseconds: Option<i64>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicePollReply {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    profile_arn: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// AWS OIDC and the desktop auth host answer a token with the same fields,
/// in AWS's camelCase.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenReply {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    profile_arn: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// What `RegisterClient` answers (v3 `login.rs`). `clientSecretExpiresAt` is
/// unix seconds and bounds every refresh the credential can make.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisteredClient {
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    client_secret_expires_at: Option<i64>,
}

/// A registered client: id, secret, and the secret's expiry when stated.
struct Registration {
    id: String,
    secret: String,
    expires_at_ms: Option<i64>,
}

impl OAuthDeviceCode for super::Kiro {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let body = json!({"clientId": CLIENT_NAME, "loginProvider": config.login_provider()});
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &format!("{}/oauth/device/authorization", config.auth_base_url()),
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let reply: DeviceCodeReply = serde_json::from_slice(&bytes)
                .map_err(|error| invalid_response(format!("device code: {error}")))?;
            let verification = reply
                .verification_uri_complete
                .clone()
                .or_else(|| reply.verification_uri.clone())
                .ok_or_else(|| invalid_response("device code: no verification uri"))?;
            Ok(DeviceAuthorization {
                device_code: reply.device_code,
                user_code: reply.user_code,
                verification_uri: verification,
                verification_uri_complete: reply.verification_uri_complete,
                expires_at_ms: reply
                    .expires_in_milliseconds
                    .filter(|milliseconds| *milliseconds > 0)
                    .or_else(|| {
                        reply
                            .expires_in
                            .filter(|seconds| *seconds > 0)
                            .map(|seconds| seconds.saturating_mul(1_000))
                    })
                    .map(|milliseconds| unix_now_ms().saturating_add(milliseconds)),
                interval_secs: reply
                    .interval_in_milliseconds
                    .map(|milliseconds| milliseconds / 1_000)
                    .or(reply.interval)
                    .map(|seconds| seconds.max(1))
                    .unwrap_or(5),
                provider_state: BTreeMap::new(),
            })
        })
    }

    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let body = json!({"deviceCode": authorization.device_code, "clientId": CLIENT_NAME});
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &format!("{}/oauth/device/poll", config.auth_base_url()),
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            let Ok(reply) = serde_json::from_slice::<DevicePollReply>(&bytes) else {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            };
            match reply.status.as_deref() {
                Some("authorization_pending") => Ok(DevicePoll::Pending),
                Some("slow_down") => Ok(DevicePoll::SlowDown {
                    interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                }),
                Some("expired_token") => Ok(DevicePoll::Expired),
                Some("authorized") => {
                    let access = required(reply.access_token, "accessToken")?;
                    let refresh = required(reply.refresh_token, "refreshToken")?;
                    let mut fields = BTreeMap::new();
                    field(&mut fields, "profile_arn", reply.profile_arn);
                    // The desktop flow has no client of its own, so there is
                    // nothing secret to seal beside the tokens.
                    Ok(DevicePoll::Ready(credential(
                        access,
                        Some(refresh),
                        reply.expires_in,
                        fields,
                        BTreeMap::new(),
                    )))
                }
                // `access_denied` and anything the desktop app has not
                // documented both mean this attempt is over.
                Some(_) => Ok(DevicePoll::Denied),
                None => Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                }),
            }
        })
    }
}

impl OAuthAuthorizationCode for super::Kiro {
    /// The AWS OIDC browser step, preceded by the dynamic client registration
    /// the Kiro CLI performs. The pair it mints belongs to this login alone, so
    /// it rides to the exchange in `provider_state`. Registration failure
    /// stops this login.
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let region = config.region()?.to_owned();
            let redirect_uri = match request.redirect_uri.trim() {
                "" => DEFAULT_REDIRECT_URI,
                uri => uri,
            };
            let registration =
                register_client(context.client, &region, redirect_uri, config.start_url())
                    .await?
                    .ok_or_else(|| {
                        ChannelError::InvalidConfig(
                            "AWS OIDC did not register a client for this login".into(),
                        )
                    })?;
            let mut provider_state = BTreeMap::new();
            provider_state.insert(CLIENT_ID.into(), Value::String(registration.id.clone()));
            provider_state.insert(CLIENT_SECRET.into(), Value::String(registration.secret));
            if let Some(expires) = registration.expires_at_ms {
                provider_state.insert(CLIENT_SECRET_EXPIRES_AT_MS.into(), Value::from(expires));
            }
            let client_id = registration.id;
            let query = form_encode(&[
                ("response_type", "code"),
                ("client_id", &client_id),
                ("redirect_uri", redirect_uri),
                ("scopes", OAUTH_SCOPE),
                ("state", request.state),
                ("code_challenge", request.code_challenge),
                ("code_challenge_method", "S256"),
            ]);
            Ok(AuthorizationStart {
                authorize_url: format!("{}/authorize?{query}", oidc_base(&region)?),
                redirect_uri: redirect_uri.to_owned(),
                provider_state,
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let (client_id, client_secret) =
                state_client(grant.provider_state).ok_or_else(|| {
                    ChannelError::InvalidConfig(
                        "OAuth login state has no registered Identity Center client".into(),
                    )
                })?;
            let region = config.region()?.to_owned();
            let body = json!({
                "grantType": "authorization_code",
                "clientId": client_id,
                "clientSecret": client_secret,
                "code": grant.code,
                "redirectUri": grant.redirect_uri,
                "codeVerifier": grant.code_verifier,
            });
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &format!("{}/token", oidc_base(&region)?),
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let reply: TokenReply = serde_json::from_slice(&bytes)
                .map_err(|error| invalid_response(format!("token: {error}")))?;
            let access = required(reply.access_token, "accessToken")?;
            let refresh = required(reply.refresh_token, "refreshToken")?;
            let mut fields = BTreeMap::new();
            // The region and the portal say which OIDC host renews this
            // credential.
            field(&mut fields, "region", Some(region));
            field(
                &mut fields,
                "start_url",
                Some(config.start_url().to_owned()),
            );
            field(&mut fields, "profile_arn", reply.profile_arn);
            let mut secrets = BTreeMap::new();
            fields.insert(CLIENT_ID.into(), Value::String(client_id));
            if let Some(expires) = grant
                .provider_state
                .get(CLIENT_SECRET_EXPIRES_AT_MS)
                .and_then(Value::as_i64)
            {
                fields.insert(CLIENT_SECRET_EXPIRES_AT_MS.into(), Value::from(expires));
            }
            secrets.insert(CLIENT_SECRET.into(), Value::String(client_secret));
            Ok(credential(
                access,
                Some(refresh),
                reply.expires_in,
                fields,
                secrets,
            ))
        })
    }
}

/// AWS OIDC `RegisterClient` (RFC 7591), as the Kiro CLI calls it before an
/// Identity Center browser login (v3 `login.rs`). `None` means the portal
/// refused — registration is disabled, or the issuer does not allow it — which
/// is an answer rather than a failure, because the operator may have registered
/// a client by hand.
async fn register_client(
    client: &dyn OutboundClient,
    region: &str,
    redirect_uri: &str,
    start_url: &str,
) -> Result<Option<Registration>, ChannelError> {
    let body = json!({
        "clientName": CLIENT_NAME,
        "clientType": "public",
        "scopes": OAUTH_SCOPE.split(' ').collect::<Vec<_>>(),
        "grantTypes": ["authorization_code", "refresh_token"],
        "redirectUris": [redirect_uri],
        "issuerUrl": start_url,
    });
    let (status, _, bytes) = send(
        client,
        Method::POST,
        &format!("{}/client/register", oidc_base(region)?),
        json_headers(),
        Some(body.to_string().into_bytes()),
    )
    .await?;
    if !status.is_success() {
        return Ok(None);
    }
    let reply: RegisteredClient = serde_json::from_slice(&bytes)
        .map_err(|error| invalid_response(format!("client registration: {error}")))?;
    Ok(Some(Registration {
        id: required(reply.client_id, "clientId")?,
        secret: required(reply.client_secret, "clientSecret")?,
        expires_at_ms: reply
            .client_secret_expires_at
            .filter(|seconds| *seconds > 0)
            .map(|seconds| seconds.saturating_mul(1_000)),
    }))
}

/// The pair `authorize` registered, as the login session parked it. Both halves
/// or neither: half a client cannot redeem anything.
fn state_client(provider_state: &BTreeMap<String, Value>) -> Option<(String, String)> {
    let value = |name: &str| {
        non_empty(provider_state.get(name))
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
    };
    value(CLIENT_ID).zip(value(CLIENT_SECRET))
}

/// The client a credential carries: the id is a public fact, so `fact` finds it
/// wherever the host put it; the secret is read out of the sealed secret alone
/// — this channel's `provider_secrets` pocket, then the flat key a v3
/// credential has. Never out of `metadata`, which is rendered: a secret found
/// there is a leak to fix, not a source to depend on.
fn own_client(credential: &CredentialView<'_>) -> Option<(String, String)> {
    let id = fact(credential, CLIENT_ID).map(str::to_owned);
    let secret = non_empty(
        credential
            .secret
            .pointer(&format!("/provider_secrets/{CLIENT_SECRET}")),
    )
    .or_else(|| non_empty(credential.secret.get(CLIENT_SECRET)))
    .map(str::to_owned);
    id.zip(secret)
}

/// When the credential's registered client stops working, if it was told.
/// Read like `fact`: host metadata, then `provider_fields`, then flat.
fn client_secret_expires_at_ms(credential: &CredentialView<'_>) -> Option<i64> {
    [
        credential.metadata.get(CLIENT_SECRET_EXPIRES_AT_MS),
        credential
            .secret
            .pointer(&format!("/provider_fields/{CLIENT_SECRET_EXPIRES_AT_MS}")),
        credential.secret.get(CLIENT_SECRET_EXPIRES_AT_MS),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_i64)
}

/// An Identity Center credential is one that names a registered client, in
/// its own credential, or that carries an Identity Center region.
fn is_identity_center(credential: &CredentialView<'_>) -> bool {
    fact(credential, CLIENT_ID).is_some() || fact(credential, "region").is_some()
}

/// The refusal codes that end a grant. AWS SSO-OIDC states one as the
/// exception name (`x-amzn-errortype`, `__type`) and as the OAuth code in
/// `error` (aws-sdk-rust `ssooidc` error shapes: `expired_token`,
/// `invalid_client`, `access_denied`, ...), so both spellings are listed.
const REFUSALS: &[&str] = &[
    "invalid_grant",
    "invalidgrant",
    "invalid_client",
    "invalidclient",
    "expired_token",
    "expiredtoken",
    "access_denied",
    "accessdenied",
    "unauthorized_client",
    "unauthorizedclient",
    "unauthorized",
];

/// Codes that only ask the caller to slow down.
const THROTTLES: &[&str] = &["slow_down", "slowdown", "throttl", "too_many", "toomany"];

/// Whether a refresh failure is definitive. Throttling never is. The Kiro
/// desktop service's `/refreshToken` refuses with a bare 400, 401 or 403,
/// which KiroCrew (`auth/refresh.py` `_REJECTED_STATUSES`) treats as a dead
/// grant. AWS OIDC is judged by the code it names, read from the
/// `x-amzn-errortype` header and the body alike; a bare 401 is definitive.
fn definitive(
    social: bool,
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> Option<String> {
    if !status.is_client_error() {
        return None;
    }
    let value: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let codes: Vec<&str> = headers
        .get("x-amzn-errortype")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(':').next().unwrap_or(value).trim())
        .into_iter()
        .chain(
            ["__type", "error", "code", "message"]
                .into_iter()
                .filter_map(|name| value.get(name).and_then(Value::as_str)),
        )
        .filter(|code| !code.is_empty())
        .collect();
    let named = |patterns: &[&str]| {
        codes.iter().copied().find(|code| {
            let lowered = code.to_ascii_lowercase();
            patterns.iter().any(|pattern| lowered.contains(pattern))
        })
    };
    if status == StatusCode::TOO_MANY_REQUESTS || named(THROTTLES).is_some() {
        return None;
    }
    if let Some(code) = named(REFUSALS) {
        return Some(code.to_owned());
    }
    let bare = if social {
        matches!(
            status,
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        )
    } else {
        status == StatusCode::UNAUTHORIZED
    };
    bare.then(|| format!("http {}", status.as_u16()))
}

impl CredentialRefresh for super::Kiro {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            let social = !is_identity_center(&context.credential);
            let (url, body) = if !social {
                // An expired registration cannot redeem anything; asking AWS
                // only earns an InvalidClient, so the refusal is local.
                if client_secret_expires_at_ms(&context.credential).is_some_and(|expires| {
                    expires <= unix_now_ms().saturating_add(EXPIRY_MARGIN_MS)
                }) {
                    return Err(ChannelError::RefreshRejected(
                        "the registered Identity Center client has expired".into(),
                    ));
                }
                let (client_id, client_secret) =
                    own_client(&context.credential).ok_or(ChannelError::InvalidCredential)?;
                let region = fact(&context.credential, "region")
                    .map(str::to_owned)
                    .unwrap_or_else(|| config.region.clone());
                (
                    format!("{}/token", oidc_base(region.trim())?),
                    json!({
                        "clientId": client_id,
                        "clientSecret": client_secret,
                        "refreshToken": refresh_token,
                        "grantType": "refresh_token",
                    }),
                )
            } else {
                (
                    format!("{}/refreshToken", config.auth_base_url()),
                    json!({ "refreshToken": refresh_token }),
                )
            };
            let (status, reply_headers, bytes) = send(
                context.client,
                Method::POST,
                &url,
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                if let Some(code) = definitive(social, status, &reply_headers, &bytes) {
                    return Err(ChannelError::RefreshRejected(code));
                }
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let reply: TokenReply = serde_json::from_slice(&bytes)
                .map_err(|error| invalid_response(format!("refresh: {error}")))?;
            let access = required(reply.access_token, "accessToken")?;
            let expires_at_ms = expires_at_ms(reply.expires_in);
            // A rotation replaces the tokens and the expiry and keeps
            // everything the reply did not name, the profile ARN above all.
            let mut updated = secret.clone();
            let object = updated
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert("access_token".into(), Value::String(access));
            if let Some(token) = reply.refresh_token.filter(|token| !token.trim().is_empty()) {
                object.insert("refresh_token".into(), Value::String(token));
            }
            object.insert("expires_at_ms".into(), Value::from(expires_at_ms));
            if let Some(arn) = reply.profile_arn.filter(|arn| !arn.trim().is_empty()) {
                let fields = object
                    .entry("provider_fields")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let Some(fields) = fields.as_object_mut() {
                    fields.insert("profile_arn".into(), Value::String(arn));
                }
            }
            Ok(CredentialUpdate {
                metadata: None,
                secret: updated,
                expires_at_ms: Some(expires_at_ms),
            })
        })
    }
}

/// The authorize URL is assembled here rather than by a URL crate, so the
/// escaping is pinned by a test.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_may_only_look_like_a_region() {
        assert!(oidc_base("us-east-1").is_ok());
        assert!(oidc_base("us-east-1/../evil").is_err());
        assert!(oidc_base("").is_err());
    }

    #[test]
    fn only_a_named_refusal_is_definitive() {
        assert!(definitive(false, StatusCode::UNAUTHORIZED, &HeaderMap::new(), b"").is_some());
        assert_eq!(
            definitive(
                false,
                StatusCode::BAD_REQUEST,
                &HeaderMap::new(),
                br#"{"__type":"InvalidGrantException"}"#
            )
            .as_deref(),
            Some("InvalidGrantException")
        );
        assert!(
            definitive(
                false,
                StatusCode::BAD_REQUEST,
                &HeaderMap::new(),
                br#"{"__type":"ThrottlingException"}"#
            )
            .is_none()
        );
        assert!(definitive(false, StatusCode::BAD_GATEWAY, &HeaderMap::new(), b"down").is_none());
    }

    #[test]
    fn the_scope_survives_being_a_query_value() {
        let query = form_encode(&[("scopes", OAUTH_SCOPE)]);
        assert_eq!(
            query,
            "scopes=codewhisperer%3Acompletions%20codewhisperer%3Aanalysis%20codewhisperer%3Aconversations"
        );
        assert_eq!(super::super::encode_component("a b"), "a%20b");
    }
}
