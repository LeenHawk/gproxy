//! The GitHub device login, and the account probe that keeps a credential
//! current.
//!
//! Signing in is GitHub's ordinary device flow with the Copilot CLI's own
//! OAuth app: `POST /login/device/code` with `client_id` and `scope`, then
//! `POST /login/oauth/access_token` with the device-code grant, both
//! form-encoded (Copilot CLI 1.0.77 `runtime.node`). What it grants is a
//! GitHub OAuth token, and that token is the whole credential: the CLI reads
//! only the reply's token (`app.js` `EMr`: `if (u.token) return u.token`),
//! so no refresh token or expiry is kept.
//!
//! The CLI then asks `GET {github_api}/copilot_internal/user` who signed in
//! (`app.js` `vQ`: `githubFetchCopilotUser(e, s, 15e3)).login`), and that
//! reply names the inference origin in `endpoints.api` (`runtime.node`
//! `CopilotUserResponse`, `endpoints.api`). The login records it; the
//! refresh is the same probe again, which re-reads the origin and finds out
//! whether GitHub still honours the token.

use super::auth;
use super::config::{CLI_USER_AGENT, CopilotCliConfig, DEFAULT_CLIENT_ID, ID};
use crate::OutboundClient;
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, CredentialView, DeviceAuthorization,
    DevicePoll, LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
    RefreshReason,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use gproxy_protocol::connection::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

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

/// GitHub's device endpoints answer JSON only when asked to.
fn form_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(header::USER_AGENT, HeaderValue::from_static(CLI_USER_AGENT));
    headers
}

#[derive(Deserialize)]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    interval: Option<u64>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// GitHub answers one body for a grant and for every pending state.
#[derive(Deserialize)]
struct AccessTokenReply {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    /// The interval a `slow_down` asks for.
    #[serde(default)]
    interval: Option<u64>,
}

/// The facts of `copilot_internal/user` this channel keeps.
#[derive(Deserialize)]
struct CopilotUser {
    #[serde(default)]
    login: Option<String>,
    #[serde(default)]
    copilot_plan: Option<String>,
    #[serde(default)]
    access_type_sku: Option<String>,
    #[serde(default)]
    endpoints: Option<Endpoints>,
}

#[derive(Deserialize)]
struct Endpoints {
    #[serde(default)]
    api: Option<String>,
}

impl CopilotUser {
    /// The public facts, as credential metadata.
    fn fields(self) -> BTreeMap<String, Value> {
        let mut fields = BTreeMap::new();
        let api = self
            .endpoints
            .and_then(|endpoints| endpoints.api)
            .and_then(|api| auth::api_origin(&api));
        for (name, value) in [
            ("login", self.login),
            ("copilot_plan", self.copilot_plan),
            ("access_type_sku", self.access_type_sku),
            (auth::API_URL, api),
        ] {
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                fields.insert(name.to_owned(), Value::String(value));
            }
        }
        fields
    }
}

fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// GitHub answers its rate limits with 403 too: an exhausted
/// `x-ratelimit-remaining`, a `retry-after`, or a body naming the limit.
fn rate_limited(headers: &HeaderMap, body: &[u8]) -> bool {
    headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "0")
        || headers.contains_key(header::RETRY_AFTER)
        || String::from_utf8_lossy(body)
            .to_ascii_lowercase()
            .contains("rate limit")
}

/// `GET copilot_internal/user` with the GitHub token, as the CLI asks it.
async fn probe(
    client: &dyn OutboundClient,
    config: &CopilotCliConfig,
    github_token: &str,
) -> Result<(StatusCode, HeaderMap, Bytes), ChannelError> {
    send(
        client,
        Method::GET,
        &format!("{}/copilot_internal/user", config.github_api_url()),
        auth::github_headers(github_token)?,
        None,
    )
    .await
}

fn user(body: &[u8]) -> Result<CopilotUser, ChannelError> {
    serde_json::from_slice(body).map_err(|error| invalid_response(format!("{ID} account: {error}")))
}

impl OAuthDeviceCode for super::CopilotCli {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = CopilotCliConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.device_authorization_url(),
                form_headers(),
                Some(form(&[
                    ("client_id", DEFAULT_CLIENT_ID),
                    ("scope", config.scope()),
                ])),
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
                verification_uri_complete: None,
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
            let config = CopilotCliConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.token_url(),
                form_headers(),
                Some(form(&[
                    ("client_id", DEFAULT_CLIENT_ID),
                    ("device_code", &authorization.device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])),
            )
            .await?;
            // GitHub answers a pending authorization with 200 and an `error`
            // code, so the body is read whatever the status says.
            let Ok(reply) = serde_json::from_slice::<AccessTokenReply>(&body) else {
                return Err(ChannelError::UpstreamResponse { status, body });
            };
            let Some(github) = reply
                .access_token
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty())
            else {
                return Ok(match reply.error.as_deref() {
                    Some("authorization_pending") => DevicePoll::Pending,
                    // The CLI waits the interval GitHub names plus five
                    // seconds, never more than a minute (`app.js` `EMr`).
                    Some("slow_down") => DevicePoll::SlowDown {
                        interval_secs: reply
                            .interval
                            .unwrap_or(authorization.interval_secs)
                            .saturating_add(5)
                            .min(60),
                    },
                    Some("access_denied") => DevicePoll::Denied,
                    Some("expired_token") => DevicePoll::Expired,
                    _ => return Err(ChannelError::UpstreamResponse { status, body }),
                });
            };
            // Then who signed in, and where their seat is served.
            let (status, _, body) = probe(context.client, &config, github).await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            Ok(DevicePoll::Ready(OAuthCredential {
                access_token: github.to_owned(),
                refresh_token: None,
                id_token: None,
                token_type: Some("Bearer".into()),
                scopes: config.scope().split(',').map(str::to_owned).collect(),
                // A GitHub OAuth app token has no stated lifetime.
                expires_at_ms: None,
                refresh_expires_at_ms: None,
                provider_fields: user(&body)?.fields(),
                provider_secrets: BTreeMap::new(),
            }))
        })
    }
}

impl CredentialRefresh for super::CopilotCli {
    /// Nothing expires on a schedule; a credential from the channel's earlier
    /// design is sent to the refresh at once, which refuses it, so it is
    /// marked for a new login rather than half-working.
    fn proactive_refresh_reason(
        &self,
        credential: CredentialView<'_>,
        _now_ms: i64,
    ) -> Option<RefreshReason> {
        auth::legacy(credential.secret).then_some(RefreshReason::ProactiveExpiry)
    }

    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            if auth::legacy(context.credential.secret) {
                return Err(ChannelError::RefreshRejected(auth::LEGACY_REASON.into()));
            }
            let config = CopilotCliConfig::from_view(context.provider)?;
            let github = auth::github_token(&context.credential).map_err(|_| {
                ChannelError::RefreshRejected("credential has no GitHub token".into())
            })?;
            let (status, headers, body) = probe(context.client, &config, github).await?;
            if !status.is_success() {
                // GitHub refusing the token is definitive: the grant was
                // revoked or the account has no Copilot seat. A 5xx is not,
                // and neither is a 403 that is GitHub's rate limit.
                let definitive = match status {
                    StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND => true,
                    StatusCode::FORBIDDEN => !rate_limited(&headers, &body),
                    _ => false,
                };
                return Err(if definitive {
                    ChannelError::RefreshRejected(format!("http {}", status.as_u16()))
                } else {
                    ChannelError::UpstreamResponse { status, body }
                });
            }
            let fields = user(&body)?.fields();
            // A full replacement that keeps everything the probe did not name.
            let mut secret = context.credential.secret.clone();
            let object = secret
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            let stored = object
                .entry("provider_fields")
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Some(stored) = stored.as_object_mut() {
                stored.extend(fields.clone());
            }
            let mut metadata = context
                .credential
                .metadata
                .as_object()
                .cloned()
                .unwrap_or_default();
            metadata.extend(fields);
            Ok(CredentialUpdate {
                secret,
                metadata: Some(Value::Object(metadata)),
                expires_at_ms: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_grant_is_form_encoded_with_its_colons_escaped() {
        assert_eq!(
            String::from_utf8(form(&[
                ("client_id", "Ov23ctDVkRmgkPke0Mmm"),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ]))
            .unwrap(),
            "client_id=Ov23ctDVkRmgkPke0Mmm\
             &grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
        );
    }

    #[test]
    fn the_probe_names_the_origin_only_when_it_is_https() {
        let fields = user(
            br#"{"login":"octo","copilot_plan":"business",
                "endpoints":{"api":"https://api.business.githubcopilot.com/"}}"#,
        )
        .unwrap()
        .fields();
        assert_eq!(fields["login"], "octo");
        assert_eq!(fields["api_url"], "https://api.business.githubcopilot.com");
        let fields = user(br#"{"endpoints":{"api":"http://evil.example"}}"#)
            .unwrap()
            .fields();
        assert!(!fields.contains_key("api_url"));
    }
}
