//! The Kimi Code device login and the refresh that keeps it alive.
//!
//! Both talk to `auth.kimi.com` in form-encoded OAuth: `POST
//! /api/oauth/device_authorization` starts, `POST /api/oauth/token` polls with
//! the device-code grant and later refreshes. The pending-authorization reply
//! is an HTTP error carrying an OAuth `error` code, so poll reads the body
//! whatever the status.
//!
//! Every one of these calls carries the CLI identity headers, device id
//! included, because the upstream binds the subscription to one machine. The
//! device id is minted here, at `start`, and travels in the credential so the
//! refresh and the operation path present the same machine.

use super::auth;
use super::config::{DEFAULT_CLIENT_ID, DEFAULT_CODE_BASE_URL, ID, KimiConfig};
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, DeviceAuthorization, DevicePoll,
    LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use http::{HeaderMap, HeaderValue, Method, header};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// What the upstream grants when it does not say.
const DEFAULT_EXPIRES_IN_SECS: i64 = 3_600;

pub(super) fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A random version-4 UUID, the shape the CLI uses for a machine id.
fn device_id() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ChannelError::InvalidConfig(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = |slice: &[u8]| {
        use std::fmt::Write as _;
        slice.iter().fold(String::new(), |mut out, byte| {
            let _ = write!(&mut out, "{byte:02x}");
            out
        })
    };
    Ok(format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    ))
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn form(pairs: &[(&str, &str)]) -> Vec<u8> {
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

/// A form post that also announces the machine. `identity` needs a credential
/// to read the device id from, and during login there is none yet, so the
/// caller passes the id it just minted.
fn login_headers(config: &KimiConfig, device: &str) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    auth::identity_with_device(&mut headers, config, device)?;
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

/// The token endpoint answers the same body for success and for every OAuth
/// error, so both arms are optional.
#[derive(Deserialize)]
struct TokenReply {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    error: Option<String>,
}

impl TokenReply {
    fn token(&self) -> Option<(&str, &str)> {
        let access = self.access_token.as_deref().filter(|t| !t.is_empty())?;
        let refresh = self.refresh_token.as_deref().filter(|t| !t.is_empty())?;
        Some((access, refresh))
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

impl OAuthDeviceCode for super::Kimi {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = KimiConfig::from_view(context.provider)?;
            let device = device_id()?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{}/api/oauth/device_authorization", config.oauth_host()),
                login_headers(&config, &device)?,
                Some(form(&[("client_id", DEFAULT_CLIENT_ID)])),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let reply: DeviceCodeReply = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} device code: {error}")))?;
            let verification = reply
                .verification_uri
                .clone()
                .or_else(|| reply.verification_uri_complete.clone())
                .ok_or_else(|| {
                    invalid_response(format!("{ID} device code: no verification uri"))
                })?;
            // The device id belongs to the session, not to the code: poll
            // must present the same machine that asked.
            let base_url = context
                .provider
                .base_url
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(DEFAULT_CODE_BASE_URL)
                .to_owned();
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
                provider_state: BTreeMap::from([
                    ("device_id".to_owned(), Value::String(device)),
                    ("base_url".to_owned(), Value::String(base_url)),
                ]),
            })
        })
    }

    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = KimiConfig::from_view(context.provider)?;
            let device = authorization
                .provider_state
                .get("device_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid_response(format!("{ID} device state has no device id")))?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{}/api/oauth/token", config.oauth_host()),
                login_headers(&config, device)?,
                Some(form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", DEFAULT_CLIENT_ID),
                    ("device_code", &authorization.device_code),
                ])),
            )
            .await?;
            // A pending authorization is an error status with an OAuth code in
            // the body, so the body is read before the status is judged.
            let Ok(reply) = serde_json::from_slice::<TokenReply>(&body) else {
                return Err(ChannelError::UpstreamResponse { status, body });
            };
            if let Some((access, refresh)) = reply.token() {
                let mut provider_fields = BTreeMap::new();
                for (name, value) in &authorization.provider_state {
                    provider_fields.insert(name.clone(), value.clone());
                }
                return Ok(DevicePoll::Ready(OAuthCredential {
                    access_token: access.to_owned(),
                    refresh_token: Some(refresh.to_owned()),
                    id_token: None,
                    token_type: Some("Bearer".into()),
                    scopes: Vec::new(),
                    expires_at_ms: Some(reply.expires_at_ms()),
                    refresh_expires_at_ms: None,
                    provider_fields,
                    provider_secrets: BTreeMap::new(),
                }));
            }
            match reply.error.as_deref() {
                Some("authorization_pending") => Ok(DevicePoll::Pending),
                Some("slow_down") => Ok(DevicePoll::SlowDown {
                    interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                }),
                Some("access_denied") => Ok(DevicePoll::Denied),
                Some("expired_token") => Ok(DevicePoll::Expired),
                _ => Err(ChannelError::UpstreamResponse { status, body }),
            }
        })
    }
}

impl CredentialRefresh for super::Kimi {
    fn supports(&self, credential: &crate::channel::CredentialView<'_>) -> bool {
        credential
            .secret
            .get("refresh_token")
            .and_then(Value::as_str)
            .is_some_and(|token| !token.trim().is_empty())
    }

    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = KimiConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            // Refreshing from a different machine than the one that logged in
            // is what the upstream is watching for; without the id, do not ask.
            let device = auth::device_id(&context.credential).ok_or_else(|| {
                ChannelError::RefreshRejected("credential has no device id".into())
            })?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{}/api/oauth/token", config.oauth_host()),
                login_headers(&config, &device)?,
                Some(form(&[
                    ("client_id", DEFAULT_CLIENT_ID),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh_token),
                ])),
            )
            .await?;
            let reply = serde_json::from_slice::<TokenReply>(&body).ok();
            if !status.is_success() || reply.as_ref().is_none_or(|reply| reply.token().is_none()) {
                let code = reply.as_ref().and_then(|reply| reply.error.clone());
                // `invalid_grant` is the upstream saying this refresh token is
                // finished; anything else may be transient.
                if status == http::StatusCode::UNAUTHORIZED
                    || matches!(code.as_deref(), Some("invalid_grant" | "invalid_client"))
                {
                    return Err(ChannelError::RefreshRejected(
                        code.unwrap_or_else(|| format!("http {}", status.as_u16())),
                    ));
                }
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let reply = reply.expect("a token reply was just confirmed");
            let (access, refresh) = reply.token().expect("a token pair was just confirmed");
            let expires_at_ms = reply.expires_at_ms();
            // A rotation replaces the two tokens and the expiry, and keeps the
            // rest of the secret — the device id above all.
            let mut updated = secret.clone();
            let object = updated
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert("access_token".into(), json!(access));
            object.insert("refresh_token".into(), json!(refresh));
            object.insert("expires_at_ms".into(), json!(expires_at_ms));
            object.insert("device_id".into(), json!(device));
            Ok(CredentialUpdate {
                secret: updated,
                expires_at_ms: Some(expires_at_ms),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_id_is_a_v4_uuid() {
        let id = device_id().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(id, device_id().unwrap());
    }

    #[test]
    fn the_device_grant_is_form_encoded_with_its_colons_escaped() {
        let body = form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", "abc"),
        ]);
        assert_eq!(
            String::from_utf8(body).unwrap(),
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=abc"
        );
    }

    #[test]
    fn a_reply_needs_both_tokens_to_count_as_one() {
        let only_access: TokenReply = serde_json::from_str(r#"{"access_token":"a"}"#).unwrap();
        assert!(only_access.token().is_none());
        let pending: TokenReply =
            serde_json::from_str(r#"{"error":"authorization_pending"}"#).unwrap();
        assert!(pending.token().is_none());
        assert_eq!(pending.error.as_deref(), Some("authorization_pending"));
        let pair: TokenReply =
            serde_json::from_str(r#"{"access_token":"a","refresh_token":"r","expires_in":60}"#)
                .unwrap();
        assert_eq!(pair.token(), Some(("a", "r")));
        assert!(pair.expires_at_ms() > unix_now_ms());
    }
}
