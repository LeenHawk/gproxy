//! Codex login flows: authorization-code + PKCE and OpenAI's custom device
//! authorization grant. Per-request token handling lives in [`super::token`].

use std::sync::Arc;

use bytes::Bytes;
use http::Request;
use http::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::channel::ChannelError;
use crate::channel::login::{DeviceInit, DevicePoll};
use crate::channel::oauth;
use crate::http::client::UpstreamClient;

/// Public Codex CLI OAuth client (the credentials the official CLI ships with).
pub(super) const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const DEFAULT_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";

const DEVICE_USERCODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

/// Build the authorize URL for the interactive authcode+PKCE login. An empty
/// redirect URI falls back to the loopback URI used by the Codex CLI.
pub(super) fn authcode_start(redirect_uri: &str, state: &str, challenge: &str) -> (String, String) {
    let redirect_uri = if redirect_uri.trim().is_empty() {
        DEFAULT_REDIRECT_URI
    } else {
        redirect_uri
    };
    let query = [
        ("response_type", "code"),
        ("client_id", OAUTH_CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", OAUTH_SCOPE),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", super::headers::ORIGINATOR),
    ]
    .iter()
    .map(|(key, value)| format!("{key}={}", percent_encode(value)))
    .collect::<Vec<_>>()
    .join("&");
    (format!("{AUTHORIZE_URL}?{query}"), redirect_uri.to_string())
}

/// Exchange an authorization code and PKCE verifier for a plaintext secret.
pub(super) async fn authcode_exchange(
    client: &Arc<dyn UpstreamClient>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<Value, ChannelError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", OAUTH_CLIENT_ID),
        ("code_verifier", verifier),
    ];
    let response = oauth::token_post(client, TOKEN_URL, &form, &[]).await?;
    super::token::secret_from_login(response)
}

fn default_interval() -> u64 {
    5
}

#[derive(Deserialize)]
struct UserCodeResponse {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(
        default = "default_interval",
        deserialize_with = "deserialize_interval"
    )]
    interval: u64,
}

fn deserialize_interval<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u64, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Number(number) => Ok(number.as_u64().unwrap_or_else(default_interval)),
        Value::String(value) => Ok(value.trim().parse().unwrap_or_else(|_| default_interval())),
        _ => Ok(default_interval()),
    }
}

#[derive(Deserialize)]
struct DevicePollResponse {
    authorization_code: String,
    code_verifier: String,
}

/// The shared device session stores one opaque string, while OpenAI polling
/// requires both values returned by the user-code endpoint.
#[derive(serde::Serialize, Deserialize)]
struct DeviceState {
    device_auth_id: String,
    user_code: String,
}

pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
) -> Result<DeviceInit, ChannelError> {
    let (status, body) = device_post(
        client,
        DEVICE_USERCODE_URL,
        &json!({ "client_id": OAUTH_CLIENT_ID }),
    )
    .await?;
    if !status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "codex deviceauth usercode {status}: {snippet}"
        )));
    }
    let response: UserCodeResponse = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Build(format!("codex usercode response parse: {error}")))?;
    let device_code = serde_json::to_string(&DeviceState {
        device_auth_id: response.device_auth_id,
        user_code: response.user_code.clone(),
    })
    .map_err(|error| ChannelError::Build(format!("codex device state serialize: {error}")))?;

    Ok(DeviceInit {
        device_code,
        user_code: response.user_code,
        verification_url: DEVICE_VERIFICATION_URL.to_string(),
        interval_secs: response.interval.max(1),
    })
}

pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    device_code: &str,
) -> Result<DevicePoll, ChannelError> {
    let state: DeviceState = serde_json::from_str(device_code)
        .map_err(|error| ChannelError::Build(format!("codex device state parse: {error}")))?;
    let (status, body) = device_post(
        client,
        DEVICE_TOKEN_URL,
        &json!({ "device_auth_id": state.device_auth_id, "user_code": state.user_code }),
    )
    .await?;

    match status.as_u16() {
        403 | 404 => Ok(DevicePoll::Pending),
        200..=299 => {
            let response: DevicePollResponse = serde_json::from_slice(&body).map_err(|error| {
                ChannelError::Build(format!("codex device token parse: {error}"))
            })?;
            let secret = authcode_exchange(
                client,
                &response.authorization_code,
                &response.code_verifier,
                DEVICE_REDIRECT_URI,
            )
            .await?;
            Ok(DevicePoll::Ready(secret))
        }
        _ => Ok(DevicePoll::Denied),
    }
}

async fn device_post(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    body: &Value,
) -> Result<(http::StatusCode, Bytes), ChannelError> {
    let payload = serde_json::to_vec(body)
        .map_err(|error| ChannelError::Build(format!("codex device request serialize: {error}")))?;
    let request = Request::builder()
        .method(http::Method::POST)
        .uri(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(payload))
        .map_err(|error| ChannelError::Build(format!("codex device request build: {error}")))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| ChannelError::Build(format!("codex device request failed: {error}")))?;
    let (parts, body) = response.into_parts();
    Ok((parts.status, body))
}

fn percent_encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(byte as char);
        } else {
            output.push('%');
            output.push(
                char::from_digit((byte >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            output.push(
                char::from_digit((byte & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    output
}
