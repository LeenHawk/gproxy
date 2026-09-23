//! The WorkOS device login Cline delegates identity to, and the refresh that
//! keeps the account token alive.
//!
//! Signing in is two exchanges, not one (v3 `cline/login.rs`). WorkOS issues
//! the device code at `POST /user_management/authorize/device` and the token
//! pair at `POST /user_management/authenticate`, both form-encoded; the pair
//! is then registered with Cline itself at `POST {base}/auth/register`, whose
//! `{success, data}` envelope carries the account token this channel actually
//! presents plus the account's own `clineUserId` and `email`. Those two are
//! facts the login discovers: they travel in `provider_fields`, the host
//! persists them, and the quota probe reads them back.
//!
//! Refreshing is Cline's own `POST {base}/auth/refresh`, not WorkOS's: the
//! account token is minted by Cline from the refresh token it handed out.

use super::auth;
use super::config::{DEFAULT_CLIENT_ID, ClineConfig, ID, base_url};
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, DeviceAuthorization, DevicePoll,
    LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use gproxy_protocol::connection::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
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

fn headers(content_type: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers
}

#[derive(Deserialize)]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// WorkOS answers the same body for a grant and for every pending state.
#[derive(Deserialize)]
struct WorkOsTokens {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl WorkOsTokens {
    fn pair(&self) -> Option<(&str, &str)> {
        let access = self.access_token.as_deref().filter(|t| !t.is_empty())?;
        let refresh = self.refresh_token.as_deref().filter(|t| !t.is_empty())?;
        Some((access, refresh))
    }
}

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The `data` of a Cline envelope, or the error it reported instead.
fn envelope(body: &Bytes) -> Result<Value, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| invalid_response(format!("{ID} envelope: {error}")))?;
    if value.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(invalid_response(
            text(value.get("error"))
                .unwrap_or("the upstream refused the registration")
                .to_owned(),
        ));
    }
    Ok(value.get("data").cloned().unwrap_or(value))
}

/// The credential a registration or a rotation establishes. `accessToken` is
/// what traffic presents; `refreshToken` is what mints the next one; the two
/// account facts are public and belong on the credential row.
fn credential(data: &Value) -> Result<OAuthCredential, ChannelError> {
    let access = text(data.get("accessToken"))
        .ok_or_else(|| invalid_response(format!("{ID}: no accessToken in the reply")))?;
    let mut provider_fields = std::collections::BTreeMap::new();
    let info = data.get("userInfo").unwrap_or(data);
    for (source, target) in [("clineUserId", "user_id"), ("email", "email")] {
        if let Some(value) = text(info.get(source)) {
            provider_fields.insert(target.to_owned(), Value::String(value.to_owned()));
        }
    }
    Ok(OAuthCredential {
        access_token: access.to_owned(),
        refresh_token: text(data.get("refreshToken")).map(str::to_owned),
        id_token: None,
        token_type: Some("Bearer".into()),
        scopes: Vec::new(),
        // Nothing on the wire dates the token; its own JWT payload does.
        expires_at_ms: auth::token_expires_at_ms(access),
        refresh_expires_at_ms: None,
        provider_fields,
        provider_secrets: std::collections::BTreeMap::new(),
    })
}

impl OAuthDeviceCode for super::Cline {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = ClineConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.device_authorization_url(),
                headers("application/x-www-form-urlencoded"),
                Some(form(&[("client_id", DEFAULT_CLIENT_ID)])),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
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
                provider_state: std::collections::BTreeMap::new(),
            })
        })
    }

    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = ClineConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.token_url(),
                headers("application/x-www-form-urlencoded"),
                Some(form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", &authorization.device_code),
                    ("client_id", DEFAULT_CLIENT_ID),
                ])),
            )
            .await?;
            // A pending authorization is an error status carrying an OAuth
            // code, so the body is read before the status is judged.
            let Ok(reply) = serde_json::from_slice::<WorkOsTokens>(&body) else {
                return Err(ChannelError::UpstreamResponse { status, body });
            };
            let Some((access, refresh)) = reply.pair() else {
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
            // WorkOS proved who the person is; Cline still has to hand out an
            // account token of its own for that identity.
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{}/auth/register", base_url(context.provider)),
                headers("application/json"),
                Some(
                    json!({ "accessToken": access, "refreshToken": refresh })
                        .to_string()
                        .into_bytes(),
                ),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            Ok(DevicePoll::Ready(credential(&envelope(&body)?)?))
        })
    }
}

impl CredentialRefresh for super::Cline {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
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
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{}/auth/refresh", base_url(context.provider)),
                headers("application/json"),
                Some(
                    json!({ "refreshToken": refresh_token, "grantType": "refresh_token" })
                        .to_string()
                        .into_bytes(),
                ),
            )
            .await?;
            let reported_failure = serde_json::from_slice::<Value>(&body)
                .ok()
                .is_some_and(|value| value.get("success").and_then(Value::as_bool) == Some(false));
            if !status.is_success() || reported_failure {
                // Only the upstream saying "this token is finished" is
                // definitive; a gateway failure is not a dead credential.
                return Err(
                    if reported_failure
                        || matches!(
                            status,
                            StatusCode::BAD_REQUEST
                                | StatusCode::UNAUTHORIZED
                                | StatusCode::FORBIDDEN
                        )
                    {
                        ChannelError::RefreshRejected(format!("http {}", status.as_u16()))
                    } else {
                        ChannelError::UpstreamResponse { status, body }
                    },
                );
            }
            let data = envelope(&body)?;
            let rotated = credential(&data)?;
            // A full replacement that keeps what the rotation did not name:
            // the refresh token when it was not rotated, and the account
            // facts the registration recorded.
            let mut secret = context.credential.secret.clone();
            let object = secret
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            // Remove only a known legacy token copy, before its old value
            // becomes indistinguishable from an independently pasted key.
            if auth::legacy_token_copy(context.credential.secret) {
                object.remove("api_key");
            }
            object.insert("access_token".into(), json!(rotated.access_token));
            if let Some(refresh) = &rotated.refresh_token {
                object.insert("refresh_token".into(), json!(refresh));
            }
            for (name, value) in &rotated.provider_fields {
                object.insert(name.clone(), value.clone());
            }
            Ok(CredentialUpdate {
                secret,
                expires_at_ms: rotated.expires_at_ms,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_escapes_the_device_grants_colons() {
        assert_eq!(
            String::from_utf8(form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", "abc"),
            ]))
            .unwrap(),
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=abc"
        );
    }

    #[test]
    fn a_registration_records_the_account_facts_it_learned() {
        let data = json!({
            "accessToken": "cline-account",
            "refreshToken": "cline-refresh",
            "userInfo": {"clineUserId": "user-1", "email": "a@b.test"},
        });
        let acquired = credential(&data).unwrap();
        assert_eq!(acquired.access_token, "cline-account");
        assert_eq!(acquired.refresh_token.as_deref(), Some("cline-refresh"));
        assert_eq!(acquired.provider_fields["user_id"], "user-1");
        assert_eq!(acquired.provider_fields["email"], "a@b.test");
    }

    #[test]
    fn a_failed_envelope_is_the_upstreams_message_and_not_a_credential() {
        let body = Bytes::from_static(br#"{"success":false,"error":"seat revoked"}"#);
        let error = envelope(&body).unwrap_err().to_string();
        assert!(error.contains("seat revoked"), "{error}");
    }
}
