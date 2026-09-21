//! The GitHub device login, and the token exchange that keeps a credential
//! usable.
//!
//! Signing in is GitHub's ordinary device flow with the Copilot CLI's own
//! OAuth app (v3 `copilotcli/login.rs`); what it grants is a long-lived
//! GitHub token, which Chat Completions does not accept. The short-lived
//! Copilot token comes from `POST
//! {github_api}/copilot_internal/v2/token`, which answers `{"token",
//! "expires_at"}` with `expires_at` in Unix seconds (v3
//! `copilotcli/auth.rs`).
//!
//! That exchange is this channel's `CredentialRefresh`: it mints a
//! short-lived credential from a long-lived one, which is exactly what a
//! refresh is, and `prepare` — being synchronous and pure — would otherwise
//! re-mint on every request. `poll` performs the first mint as the final
//! exchange of the login, so the credential the host persists is usable
//! straight away.

use super::auth;
use super::config::{CopilotCliConfig, ID};
use crate::OutboundClient;
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, DeviceAuthorization, DevicePoll,
    LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::json;
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
}

/// The Copilot token mint's reply. `expires_at` is Unix seconds.
#[derive(Deserialize)]
struct CopilotTokenReply {
    token: String,
    expires_at: i64,
}

fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Exchange the GitHub token for a Copilot token. Shared by the login's final
/// step and by every later refresh, because they are the same exchange.
async fn mint(
    client: &dyn OutboundClient,
    config: &CopilotCliConfig,
    github_token: &str,
) -> Result<(String, i64), ChannelError> {
    let (status, _, body) = send(
        client,
        Method::GET,
        &format!("{}/copilot_internal/v2/token", config.github_api_url()),
        auth::github_headers(config, github_token)?,
        None,
    )
    .await?;
    if !status.is_success() {
        // GitHub refusing the token itself is definitive: the OAuth grant was
        // revoked or the account lost its Copilot seat. A 5xx is not.
        return Err(
            if matches!(
                status,
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
            ) {
                ChannelError::RefreshRejected(format!("http {}", status.as_u16()))
            } else {
                ChannelError::UpstreamResponse { status, body }
            },
        );
    }
    let reply: CopilotTokenReply = serde_json::from_slice(&body)
        .map_err(|error| invalid_response(format!("{ID} token: {error}")))?;
    if reply.token.trim().is_empty() {
        return Err(invalid_response(format!("{ID} token: empty token")));
    }
    Ok((reply.token, reply.expires_at.saturating_mul(1_000)))
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
                    ("client_id", config.client_id()),
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
                    ("client_id", config.client_id()),
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
                    Some("slow_down") => DevicePoll::SlowDown {
                        interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                    },
                    Some("access_denied") => DevicePoll::Denied,
                    Some("expired_token") => DevicePoll::Expired,
                    _ => return Err(ChannelError::UpstreamResponse { status, body }),
                });
            };
            // The GitHub token is not what inference accepts, so the login
            // spends it once here; every later mint is the refresh.
            let (copilot, expires_at_ms) = mint(context.client, &config, github).await?;
            Ok(DevicePoll::Ready(OAuthCredential {
                access_token: copilot,
                refresh_token: Some(github.to_owned()),
                id_token: None,
                token_type: Some("Bearer".into()),
                scopes: vec![config.scope().to_owned()],
                expires_at_ms: Some(expires_at_ms),
                // The GitHub token has no stated lifetime of its own.
                refresh_expires_at_ms: None,
                provider_fields: BTreeMap::new(),
                provider_secrets: BTreeMap::new(),
            }))
        })
    }
}

impl CredentialRefresh for super::CopilotCli {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = CopilotCliConfig::from_view(context.provider)?;
            let github = auth::github_token(&context.credential).map_err(|_| {
                ChannelError::RefreshRejected("credential has no GitHub token".into())
            })?;
            let (copilot, expires_at_ms) = mint(context.client, &config, github).await?;
            // A full replacement that keeps what the mint did not name: the
            // GitHub token above all, and any seat the host recorded.
            let mut secret = context.credential.secret.clone();
            let object = secret
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            // v3's names are kept in step when the secret still uses them, so
            // a v3 credential never ends up holding two disagreeing pairs.
            let v3_names =
                object.contains_key("copilot_token") || object.contains_key("github_token");
            object.insert("access_token".into(), json!(copilot));
            object.insert("refresh_token".into(), json!(github));
            object.insert("expires_at_ms".into(), json!(expires_at_ms));
            if v3_names {
                object.insert("copilot_token".into(), json!(copilot));
                object.insert("github_token".into(), json!(github));
                object.insert("copilot_expires_at_ms".into(), json!(expires_at_ms));
            }
            Ok(CredentialUpdate {
                secret,
                expires_at_ms: Some(expires_at_ms),
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
                ("client_id", "Iv1.b507a08c87ecfe98"),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ]))
            .unwrap(),
            "client_id=Iv1.b507a08c87ecfe98\
             &grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
        );
    }

    #[test]
    fn the_mint_reports_seconds_and_the_channel_stores_milliseconds() {
        let reply: CopilotTokenReply =
            serde_json::from_str(r#"{"token":"tid=x","expires_at":1800000000}"#).unwrap();
        assert_eq!(reply.expires_at.saturating_mul(1_000), 1_800_000_000_000);
    }
}
