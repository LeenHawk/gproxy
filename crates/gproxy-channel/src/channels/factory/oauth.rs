//! The Droid CLI's WorkOS login and refresh.
//!
//! droid 1.18.0 (`x/js/0070.js`): `POST {workos}/authorize/device` with the
//! form `client_id` starts; `POST {workos}/authenticate` with
//! `grant_type=urn:ietf:params:oauth:grant-type:device_code` polls, answering
//! `authorization_pending`, `slow_down`, `access_denied` or `expired_token`
//! until it grants `{access_token, refresh_token}`. The refresh is the same
//! endpoint with `grant_type=refresh_token` and, when the credential acts in
//! one, `organization_id`. The access token is a JWT whose `exp` is its
//! lifetime; WorkOS states no `expires_in`.

use super::config::{FactoryConfig, ID};
use super::request::fact;
use crate::OutboundClient;
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use base64::Engine as _;
use gproxy_upstream::{
    CredentialRefresh, CredentialUpdate, CredentialView, DeviceAuthorization, DevicePoll,
    LoginContext, OAuthCredential, OAuthDeviceCode, RefreshContext, UpstreamError, UpstreamFuture,
};
use http::{HeaderMap, HeaderValue, Method, header};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// The CLI adds one second per `slow_down`.
const SLOW_DOWN_STEP_SECS: u64 = 1;

fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn form_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers
}

fn form(pairs: &[(&str, &str)]) -> Vec<u8> {
    fn escape(text: &str) -> String {
        text.bytes()
            .map(|byte| match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (byte as char).to_string()
                }
                _ => format!("%{byte:02X}"),
            })
            .collect()
    }
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", escape(key), escape(value)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

/// The `exp` of a JWT access token, in milliseconds.
fn jwt_expiry_ms(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims
        .get("exp")
        .and_then(Value::as_i64)
        .map(|seconds| seconds.saturating_mul(1_000))
}

/// The organization a JWT access token acts in (`org_id`), when it names one.
fn jwt_org(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims
        .get("org_id")
        .and_then(Value::as_str)
        .filter(|org| !org.is_empty())
        .map(str::to_owned)
}

#[derive(Deserialize)]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct User {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

/// WorkOS answers the grant, every pending state and a refusal in one shape.
#[derive(Deserialize)]
struct TokenReply {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    organization_id: Option<String>,
    #[serde(default)]
    user: Option<User>,
    #[serde(default)]
    error: Option<String>,
}

impl TokenReply {
    fn access(&self) -> Option<&str> {
        self.access_token.as_deref().filter(|t| !t.trim().is_empty())
    }

    fn refresh(&self) -> Option<&str> {
        self.refresh_token.as_deref().filter(|t| !t.trim().is_empty())
    }

    fn organization(&self, access: &str) -> Option<String> {
        self.organization_id
            .clone()
            .filter(|org| !org.is_empty())
            .or_else(|| jwt_org(access))
    }
}

async fn authenticate(
    client: &dyn OutboundClient,
    config: &FactoryConfig,
    pairs: &[(&str, &str)],
) -> Result<(http::StatusCode, gproxy_protocol::connection::Bytes), UpstreamError> {
    let (status, _, body) = send(
        client,
        Method::POST,
        &format!("{}/authenticate", config.workos_base_url()),
        form_headers(),
        Some(form(pairs)),
    )
    .await?;
    Ok((status, body))
}

impl OAuthDeviceCode for super::Factory {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> UpstreamFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = FactoryConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{}/authorize/device", config.workos_base_url()),
                form_headers(),
                Some(form(&[("client_id", config.client_id())])),
            )
            .await?;
            if !status.is_success() {
                return Err(UpstreamError::UpstreamResponse { status, body });
            }
            let reply: DeviceCodeReply = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} device code: {error}")))?;
            Ok(DeviceAuthorization {
                device_code: reply.device_code,
                user_code: reply.user_code,
                verification_uri: reply.verification_uri,
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
    ) -> UpstreamFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = FactoryConfig::from_view(context.provider)?;
            let (status, body) = authenticate(
                context.client,
                &config,
                &[
                    ("grant_type", DEVICE_GRANT),
                    ("device_code", &authorization.device_code),
                    ("client_id", config.client_id()),
                ],
            )
            .await?;
            // A pending authorization is an error status carrying an OAuth
            // code, so the body is read before the status is judged.
            let Ok(reply) = serde_json::from_slice::<TokenReply>(&body) else {
                return Err(UpstreamError::UpstreamResponse { status, body });
            };
            let Some(access) = reply.access() else {
                return Ok(match reply.error.as_deref() {
                    Some("authorization_pending") => DevicePoll::Pending,
                    Some("slow_down") => DevicePoll::SlowDown {
                        interval_secs: authorization
                            .interval_secs
                            .saturating_add(SLOW_DOWN_STEP_SECS),
                    },
                    Some("access_denied") => DevicePoll::Denied,
                    Some("expired_token") => DevicePoll::Expired,
                    _ => return Err(UpstreamError::UpstreamResponse { status, body }),
                });
            };
            let mut provider_fields = BTreeMap::new();
            if let Some(org) = reply.organization(access) {
                provider_fields.insert("organization_id".into(), Value::String(org));
            }
            if let Some(user) = &reply.user {
                if let Some(id) = user.id.clone().filter(|id| !id.is_empty()) {
                    provider_fields.insert("user_id".into(), Value::String(id));
                }
                if let Some(email) = user.email.clone().filter(|email| !email.is_empty()) {
                    provider_fields.insert("email".into(), Value::String(email));
                }
            }
            Ok(DevicePoll::Ready(OAuthCredential {
                access_token: access.to_owned(),
                refresh_token: reply.refresh().map(str::to_owned),
                id_token: None,
                token_type: Some("Bearer".into()),
                scopes: Vec::new(),
                expires_at_ms: jwt_expiry_ms(access),
                refresh_expires_at_ms: None,
                provider_fields,
                provider_secrets: BTreeMap::new(),
            }))
        })
    }
}

fn refresh_token<'a>(credential: &CredentialView<'a>) -> Option<&'a str> {
    credential
        .secret
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

impl CredentialRefresh for super::Factory {
    /// A pasted API key does not expire; only a WorkOS login refreshes.
    fn supports(&self, credential: &CredentialView<'_>) -> bool {
        refresh_token(credential).is_some()
    }

    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> UpstreamFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = FactoryConfig::from_view(context.provider)?;
            let refresh = refresh_token(&context.credential).ok_or_else(|| {
                UpstreamError::RefreshRejected("credential has no refresh token".into())
            })?;
            let organization = fact(&context.credential, "organization_id");
            let mut pairs = vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", config.client_id()),
            ];
            if let Some(org) = organization {
                pairs.push(("organization_id", org));
            }
            let (status, body) = authenticate(context.client, &config, &pairs).await?;
            let reply = serde_json::from_slice::<TokenReply>(&body).ok();
            let granted = reply.as_ref().and_then(TokenReply::access).map(str::to_owned);
            let Some(access) = granted.filter(|_| status.is_success()) else {
                // WorkOS refuses a spent, revoked or foreign refresh token with
                // `invalid_grant`; anything else may pass.
                let code = reply.and_then(|reply| reply.error);
                if code.as_deref() == Some("invalid_grant") {
                    return Err(UpstreamError::RefreshRejected("invalid_grant".into()));
                }
                return Err(UpstreamError::UpstreamResponse { status, body });
            };
            let reply = reply.expect("a token reply was just confirmed");
            let expires_at_ms = jwt_expiry_ms(&access);
            let mut secret = context.credential.secret.clone();
            let object = secret
                .as_object_mut()
                .ok_or(UpstreamError::InvalidCredential)?;
            object.insert("access_token".into(), json!(access));
            if let Some(rotated) = reply.refresh() {
                object.insert("refresh_token".into(), json!(rotated));
            }
            match expires_at_ms {
                Some(expiry) => {
                    object.insert("expires_at_ms".into(), json!(expiry));
                }
                None => {
                    object.remove("expires_at_ms");
                }
            }
            Ok(CredentialUpdate {
                metadata: None,
                secret,
                expires_at_ms,
            })
        })
    }
}
