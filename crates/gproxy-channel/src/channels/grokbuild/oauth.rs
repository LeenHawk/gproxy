//! The xAI device login the Grok Build CLI uses, and the refresh that keeps
//! it alive (v3 `grokbuild/{login,auth}.rs`).
//!
//! Both are form-encoded OAuth at `auth.x.ai`: `POST /oauth2/device/code`
//! starts, `POST /oauth2/token` polls with the device-code grant and later
//! refreshes. The token endpoint answers the same body for success and for
//! every OAuth error, so the body is read whatever the status.
//!
//! The id token is the only place the account's subject appears, and
//! `x-grok-user-id` needs it on every request. It is decoded once, here, and
//! travels in `provider_fields` — a login fact the host persists, not
//! something `prepare` reparses.

use super::config::{DEFAULT_CLIENT_ID, GrokBuildConfig, ID, OAUTH_SCOPE};
use super::{form_encode, unix_now_ms};
use crate::OutboundClient;
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, DeviceAuthorization, DevicePoll,
    LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use crate::channels::shared::timer::sleep;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http::{HeaderMap, HeaderValue, Method, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// What the upstream grants when it does not say.
const DEFAULT_EXPIRES_IN_SECS: i64 = 3_600;

/// The CLI's shared reqwest client: its process user agent, `.form()`'s
/// content type and reqwest's default `Accept` (`xai-grok-http`
/// `shared_client`). The refresh sends nothing else.
fn form_headers() -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_str(&super::auth::user_agent(false))
            .map_err(|_| ChannelError::InvalidConfig("invalid CLI user agent".into()))?,
    );
    Ok(headers)
}

/// The device flow adds the client version and the surface the login is
/// finished on (CLI `device_code.rs`). A console login renders the code to a
/// person, which the CLI calls `ui`.
fn device_headers() -> Result<HeaderMap, ChannelError> {
    let mut headers = form_headers()?;
    headers.insert(
        "x-grok-client-version",
        HeaderValue::from_static(super::auth::CLI_VERSION),
    );
    headers.insert("x-grok-client-surface", HeaderValue::from_static("ui"));
    Ok(headers)
}

#[derive(Deserialize)]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    #[serde(default)]
    verification_uri: Option<String>,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct TokenReply {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    error: Option<String>,
}

impl TokenReply {
    fn access(&self) -> Option<&str> {
        self.access_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
    }

    fn expires_at_ms(&self) -> i64 {
        unix_now_ms().saturating_add(
            self.expires_in
                .filter(|seconds| *seconds > 0)
                .unwrap_or(DEFAULT_EXPIRES_IN_SECS)
                .saturating_mul(1_000),
        )
    }
}

/// One claim of an unverified id token. The channel reads it for the account
/// facts it needs to present, never to decide anything about trust.
pub(super) fn jwt_claim(token: &str, name: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get(name)?
        .as_str()
        .map(str::to_owned)
}

/// The principal the consent screen chose, peeked from the access token as
/// the CLI's `oidc::peek_access_token_principal` does: both claims, either
/// spelling, neither empty.
fn principal(access_token: &str) -> Option<(String, String)> {
    let claim = |snake: &str, camel: &str| {
        jwt_claim(access_token, snake)
            .or_else(|| jwt_claim(access_token, camel))
            .filter(|value| !value.is_empty())
    };
    Some((
        claim("principal_type", "principalType")?,
        claim("principal_id", "principalId")?,
    ))
}

/// A Team or Organization principal is its own user and has no email; the
/// consent screen strips `openid` and `email` for it (CLI
/// `device_code.rs::build_auth`).
fn principal_is_account(kind: &str) -> bool {
    matches!(kind, "Team" | "Organization")
}

/// The account facts: the principal out of the access token, then `sub` and
/// `email` out of the id token unless the principal is a team or an
/// organization, whose id is the user id the CLI presents.
fn account_fields(
    access_token: &str,
    id_token: Option<&str>,
    fields: &mut BTreeMap<String, Value>,
) {
    if let Some((kind, id)) = principal(access_token) {
        fields.insert("principal_type".into(), Value::String(kind.clone()));
        fields.insert("principal_id".into(), Value::String(id.clone()));
        if principal_is_account(&kind) {
            fields.insert("sub".into(), Value::String(id));
            return;
        }
    }
    let Some(id_token) = id_token.map(str::trim).filter(|token| !token.is_empty()) else {
        return;
    };
    if let Some(subject) = jwt_claim(id_token, "sub") {
        fields.insert("sub".into(), Value::String(subject));
    }
    if let Some(email) = jwt_claim(id_token, "email") {
        fields.insert("user_email".into(), Value::String(email));
    }
}

impl OAuthDeviceCode for super::GrokBuild {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = GrokBuildConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.device_code_url(),
                device_headers()?,
                Some(
                    form_encode(&[
                        ("client_id", DEFAULT_CLIENT_ID),
                        ("scope", OAUTH_SCOPE),
                        ("referrer", "grok-build"),
                    ])
                    .into_bytes(),
                ),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let reply: DeviceCodeReply = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} device code: {error}")))?;
            let verification = reply
                .verification_uri_complete
                .clone()
                .or_else(|| reply.verification_uri.clone())
                .ok_or_else(|| {
                    invalid_response(format!("{ID} device code: no verification uri"))
                })?;
            Ok(DeviceAuthorization {
                device_code: reply.device_code,
                user_code: reply.user_code,
                verification_uri: verification,
                verification_uri_complete: reply.verification_uri_complete,
                expires_at_ms: reply
                    .expires_in
                    .filter(|seconds| *seconds > 0)
                    .map(|seconds| unix_now_ms().saturating_add(seconds.saturating_mul(1_000))),
                interval_secs: reply.interval.unwrap_or(5).max(1),
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
            let config = GrokBuildConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.token_url(),
                device_headers()?,
                Some(
                    form_encode(&[
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                        ("device_code", &authorization.device_code),
                        ("client_id", DEFAULT_CLIENT_ID),
                    ])
                    .into_bytes(),
                ),
            )
            .await?;
            // A pending authorization is an error status with an OAuth code
            // in the body, so the body is read before the status is judged.
            let Ok(reply) = serde_json::from_slice::<TokenReply>(&body) else {
                return Err(ChannelError::UpstreamResponse { status, body });
            };
            if let Some(access) = reply.access() {
                let mut fields = BTreeMap::new();
                account_fields(access, reply.id_token.as_deref(), &mut fields);
                return Ok(DevicePoll::Ready(OAuthCredential {
                    access_token: access.to_owned(),
                    refresh_token: reply
                        .refresh_token
                        .clone()
                        .filter(|token| !token.trim().is_empty()),
                    id_token: reply.id_token.clone(),
                    token_type: Some("Bearer".into()),
                    scopes: OAUTH_SCOPE.split(' ').map(str::to_owned).collect(),
                    expires_at_ms: Some(reply.expires_at_ms()),
                    refresh_expires_at_ms: None,
                    provider_fields: fields,
                    provider_secrets: BTreeMap::new(),
                }));
            }
            match reply.error.as_deref() {
                Some("authorization_pending") => Ok(DevicePoll::Pending),
                // The CLI adds five seconds per `slow_down`.
                Some("slow_down") => Ok(DevicePoll::SlowDown {
                    interval_secs: authorization.interval_secs.saturating_add(5),
                }),
                Some("access_denied") => Ok(DevicePoll::Denied),
                Some("expired_token") => Ok(DevicePoll::Expired),
                _ => Err(ChannelError::UpstreamResponse { status, body }),
            }
        })
    }
}

/// Attempts the CLI makes per refresh (`oidc/protocol.rs::refresh_retry_policy`:
/// one try and two retries, 200ms–2s jittered exponential backoff).
const REFRESH_ATTEMPTS: u32 = 3;
const REFRESH_MIN_DELAY_MS: u64 = 200;
const REFRESH_MAX_DELAY_MS: u64 = 2_000;

/// One refresh exchange: a reply carrying an access token, or the error the
/// attempt ended in.
async fn refresh_once(
    client: &dyn OutboundClient,
    url: &str,
    form: &str,
) -> Result<TokenReply, ChannelError> {
    let (status, _, body) = send(
        client,
        Method::POST,
        url,
        form_headers()?,
        Some(form.as_bytes().to_vec()),
    )
    .await?;
    let reply = serde_json::from_slice::<TokenReply>(&body).ok();
    if status.is_success() && reply.as_ref().is_some_and(|reply| reply.access().is_some()) {
        return Ok(reply.expect("a token reply was just confirmed"));
    }
    // Only the OAuth code is definitive, as in the CLI's
    // `oidc/refresh.rs::classify_terminal`: a bare 4xx, 429 or 5xx is
    // retried there, never a dead credential.
    match reply.as_ref().and_then(|reply| reply.error.as_deref()) {
        Some(code @ ("invalid_grant" | "invalid_client")) => {
            Err(ChannelError::RefreshRejected(code.to_owned()))
        }
        _ => Err(ChannelError::UpstreamResponse { status, body }),
    }
}

/// The CLI's retry around the exchange: a transport failure or any
/// non-terminal reply is tried again; a terminal code stops at once.
async fn refresh_with_retries(
    client: &dyn OutboundClient,
    url: &str,
    form: String,
) -> Result<TokenReply, ChannelError> {
    let mut attempt = 0;
    loop {
        let error = match refresh_once(client, url, &form).await {
            Ok(reply) => return Ok(reply),
            Err(error @ (ChannelError::Transport(_) | ChannelError::UpstreamResponse { .. })) => {
                error
            }
            Err(error) => return Err(error),
        };
        attempt += 1;
        if attempt >= REFRESH_ATTEMPTS {
            return Err(error);
        }
        let delay = REFRESH_MIN_DELAY_MS
            .saturating_mul(1 << (attempt - 1))
            .min(REFRESH_MAX_DELAY_MS);
        let mut bytes = [0u8; 8];
        let jitter = match getrandom::fill(&mut bytes) {
            Ok(()) => u64::from_le_bytes(bytes) % delay,
            Err(_) => 0,
        };
        sleep(Duration::from_millis(delay + jitter)).await;
    }
}

impl CredentialRefresh for super::GrokBuild {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = GrokBuildConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            // A credential minted against a different issuer keeps renewing
            // against that one.
            let url = super::auth::fact(&context.credential, "token_endpoint")
                .unwrap_or(config.token_url())
                .to_owned();
            let mut form = vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", DEFAULT_CLIENT_ID),
            ];
            // The principal the login chose renews as that principal (CLI
            // `oidc/protocol.rs::refresh_tokens_once`).
            for name in ["principal_type", "principal_id"] {
                if let Some(value) = super::auth::fact(&context.credential, name) {
                    form.push((name, value));
                }
            }
            let reply = refresh_with_retries(context.client, &url, form_encode(&form)).await?;
            let access = reply.access().expect("an access token was just confirmed");
            let expires_at_ms = reply.expires_at_ms();
            // A rotation replaces the tokens and the expiry and keeps
            // everything the reply did not name.
            let mut updated = secret.clone();
            let object = updated
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert("access_token".into(), Value::String(access.to_owned()));
            if let Some(token) = reply
                .refresh_token
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty())
            {
                object.insert("refresh_token".into(), Value::String(token.to_owned()));
            }
            object.insert("expires_at_ms".into(), Value::from(expires_at_ms));
            let id_token = reply
                .id_token
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty());
            if let Some(id_token) = id_token {
                object.insert("id_token".into(), Value::String(id_token.to_owned()));
            }
            // The account facts stay the login's: the CLI reuses that
            // identity and skips the refreshed id token
            // (`oidc/refresh.rs::oidc_token_exchange`).
            Ok(CredentialUpdate {
                metadata: None,
                secret: updated,
                expires_at_ms: Some(expires_at_ms),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `{"sub":"user-1","email":"a@b.c"}`, unsigned and unpadded.
    fn id_token() -> String {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"user-1","email":"a@b.c"}"#);
        format!("header.{payload}.signature")
    }

    #[test]
    fn the_subject_and_the_email_come_out_of_the_id_token() {
        let mut fields = BTreeMap::new();
        account_fields("opaque", Some(&id_token()), &mut fields);
        assert_eq!(fields["sub"], "user-1");
        assert_eq!(fields["user_email"], "a@b.c");

        let mut none = BTreeMap::new();
        account_fields("opaque", Some("not.a.jwt"), &mut none);
        assert!(none.is_empty(), "an unreadable token adds nothing");
    }

    #[test]
    fn the_device_grant_is_form_encoded_with_its_colons_escaped() {
        let body = form_encode(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", "abc"),
        ]);
        assert_eq!(
            body,
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=abc"
        );
    }
}
