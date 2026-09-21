//! The device login the `opencode` CLI performs against the Console, and the
//! refresh that shares its endpoint.
//!
//! Both are JSON, not form-encoded: `POST {console}/auth/device/code` with
//! `{client_id}` starts, `POST {console}/auth/device/token` both polls the
//! device-code grant and later refreshes (v3 `opencode/login.rs`,
//! `opencode/auth.rs`). The console origin travels in `provider_fields` so a
//! refresh returns to the same one the login used.

use super::config::{ID, OpenCodeConfig};
use super::request::fact;
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, DeviceAuthorization, DevicePoll,
    LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Where the console origin is recorded on a credential.
const CONSOLE_FIELD: &str = "console_base_url";

fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers
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

/// The token endpoint answers one body for a grant, for a refusal and for
/// every pending state.
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
    fn access(&self) -> Option<&str> {
        self.access_token
            .as_deref()
            .filter(|t| !t.trim().is_empty())
    }

    fn refresh(&self) -> Option<&str> {
        self.refresh_token
            .as_deref()
            .filter(|t| !t.trim().is_empty())
    }

    /// An expiry only when the upstream stated a lifetime; it does not always
    /// do so, and an invented one would have the host refresh a live token.
    fn expires_at_ms(&self) -> Option<i64> {
        self.expires_in
            .filter(|seconds| *seconds > 0)
            .map(|seconds| unix_now_ms().saturating_add(seconds.saturating_mul(1_000)))
    }
}

/// A relative `verification_uri` is relative to the console.
fn verification(base: &str, path: Option<String>) -> String {
    let path = path.unwrap_or_else(|| "/device".into());
    if path.starts_with("http") {
        path
    } else {
        format!("{base}{path}")
    }
}

impl OAuthDeviceCode for super::OpenCode {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = OpenCodeConfig::from_view(context.provider)?;
            let base = config.console_base_url(None);
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{base}/auth/device/code"),
                json_headers(),
                Some(
                    json!({ "client_id": config.client_id() })
                        .to_string()
                        .into_bytes(),
                ),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let reply: DeviceCodeReply = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} device code: {error}")))?;
            let complete = reply
                .verification_uri_complete
                .clone()
                .map(|path| verification(&base, Some(path)));
            Ok(DeviceAuthorization {
                device_code: reply.device_code,
                user_code: reply.user_code,
                verification_uri: verification(&base, reply.verification_uri),
                verification_uri_complete: complete,
                expires_at_ms: reply
                    .expires_in
                    .filter(|seconds| *seconds > 0)
                    .map(|seconds| unix_now_ms().saturating_add(seconds.saturating_mul(1_000))),
                interval_secs: reply.interval.unwrap_or(5).max(1),
                // The console the code was minted at is the console the poll
                // and every later refresh must return to.
                provider_state: BTreeMap::from([(CONSOLE_FIELD.to_owned(), Value::String(base))]),
            })
        })
    }

    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = OpenCodeConfig::from_view(context.provider)?;
            let base = config.console_base_url(
                authorization
                    .provider_state
                    .get(CONSOLE_FIELD)
                    .and_then(Value::as_str),
            );
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{base}/auth/device/token"),
                json_headers(),
                Some(
                    json!({
                        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                        "device_code": authorization.device_code,
                        "client_id": config.client_id(),
                    })
                    .to_string()
                    .into_bytes(),
                ),
            )
            .await?;
            // A pending authorization is an error status carrying an OAuth
            // code, so the body is read before the status is judged.
            let Ok(reply) = serde_json::from_slice::<TokenReply>(&body) else {
                return Err(ChannelError::UpstreamResponse { status, body });
            };
            let Some(access) = reply.access() else {
                return Ok(match reply.error.as_deref() {
                    Some("authorization_pending") => DevicePoll::Pending,
                    Some("slow_down") => DevicePoll::SlowDown {
                        interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                    },
                    Some("access_denied" | "invalid_grant") => DevicePoll::Denied,
                    Some("expired_token") => DevicePoll::Expired,
                    _ => return Err(ChannelError::UpstreamResponse { status, body }),
                });
            };
            Ok(DevicePoll::Ready(OAuthCredential {
                access_token: access.to_owned(),
                refresh_token: reply.refresh().map(str::to_owned),
                id_token: None,
                token_type: Some("Bearer".into()),
                scopes: Vec::new(),
                expires_at_ms: reply.expires_at_ms(),
                refresh_expires_at_ms: None,
                provider_fields: BTreeMap::from([(CONSOLE_FIELD.to_owned(), Value::String(base))]),
                provider_secrets: BTreeMap::new(),
            }))
        })
    }
}

impl CredentialRefresh for super::OpenCode {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = OpenCodeConfig::from_view(context.provider)?;
            let refresh_token = context
                .credential
                .secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            let base = config.console_base_url(fact(&context.credential, CONSOLE_FIELD));
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{base}/auth/device/token"),
                json_headers(),
                Some(
                    json!({
                        "grant_type": "refresh_token",
                        "refresh_token": refresh_token,
                        "client_id": config.client_id(),
                    })
                    .to_string()
                    .into_bytes(),
                ),
            )
            .await?;
            let reply = serde_json::from_slice::<TokenReply>(&body).ok();
            let granted = reply
                .as_ref()
                .and_then(TokenReply::access)
                .map(str::to_owned);
            if !status.is_success() || granted.is_none() {
                let code = reply.as_ref().and_then(|reply| reply.error.clone());
                // `invalid_grant` is the upstream saying this refresh token is
                // finished; anything else may be transient.
                if status == StatusCode::UNAUTHORIZED
                    || matches!(code.as_deref(), Some("invalid_grant" | "invalid_client"))
                {
                    return Err(ChannelError::RefreshRejected(
                        code.unwrap_or_else(|| format!("http {}", status.as_u16())),
                    ));
                }
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let reply = reply.expect("a token reply was just confirmed");
            let access = granted.expect("an access token was just confirmed");
            let expires_at_ms = reply.expires_at_ms();
            // A full replacement that keeps what the rotation did not name,
            // the console origin above all.
            let mut secret = context.credential.secret.clone();
            let object = secret
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert("access_token".into(), json!(access));
            // A pasted key and an account token are the same field upstream;
            // v3 kept both names in step and so does this.
            object.insert("api_key".into(), json!(access));
            if let Some(rotated) = reply.refresh() {
                object.insert("refresh_token".into(), json!(rotated));
            }
            match expires_at_ms {
                Some(expiry) => {
                    object.insert("expires_at_ms".into(), json!(expiry));
                }
                // The upstream stated no lifetime; a stale expiry would be a
                // lie, so it is removed rather than kept.
                None => {
                    object.remove("expires_at_ms");
                }
            }
            object.insert(CONSOLE_FIELD.into(), json!(base));
            Ok(CredentialUpdate {
                secret,
                expires_at_ms,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_verification_uri_hangs_off_the_console() {
        assert_eq!(
            verification("https://console.example", Some("/device".into())),
            "https://console.example/device"
        );
        assert_eq!(
            verification("https://console.example", Some("https://other/x".into())),
            "https://other/x"
        );
        assert_eq!(
            verification(super::super::config::DEFAULT_CONSOLE_BASE_URL, None),
            "https://console.opencode.ai/device"
        );
    }

    #[test]
    fn a_reply_without_a_lifetime_states_no_expiry() {
        let stated: TokenReply =
            serde_json::from_str(r#"{"access_token":"a","expires_in":60}"#).unwrap();
        assert!(stated.expires_at_ms().unwrap() > unix_now_ms());
        let silent: TokenReply = serde_json::from_str(r#"{"access_token":"a"}"#).unwrap();
        assert_eq!(silent.expires_at_ms(), None);
        let pending: TokenReply =
            serde_json::from_str(r#"{"error":"authorization_pending"}"#).unwrap();
        assert!(pending.access().is_none());
    }
}
