//! Kimi Code device OAuth and managed-endpoint identity.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use http::{HeaderMap, Request};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::channel::{ChannelError, DeviceInit, DevicePoll};
use crate::http::client::UpstreamClient;

pub(super) const DEFAULT_BASE_URL: &str = "https://api.kimi.com/coding/v1";
pub(super) const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const CLI_VERSION: &str = "0.36.1";
const PLATFORM: &str = "kimi_code_cli";
const USER_AGENT_VALUE: &str = "kimi-code-cli/0.36.1";
const EXPIRY_SKEW_MS: i64 = 60_000;
const DEVICE_STATE_PREFIX: &str = "kimicode:";

#[derive(Debug, Serialize, Deserialize)]
struct PendingDevice {
    device_code: String,
    device_id: String,
    oauth_host: String,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri_complete: String,
    #[serde(default = "default_interval")]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct TokenPayload {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

fn default_interval() -> u64 {
    5
}

pub(super) fn base_url<'a>(settings: &'a Value, secret: &'a Value) -> &'a str {
    setting(settings, "base_url")
        .or_else(|| field(secret, "base_url"))
        .unwrap_or(DEFAULT_BASE_URL)
}

fn oauth_host<'a>(settings: &'a Value, secret: Option<&'a Value>) -> &'a str {
    setting(settings, "oauth_host")
        .or_else(|| secret.and_then(|value| field(value, "oauth_host")))
        .unwrap_or(DEFAULT_OAUTH_HOST)
}

pub(super) fn upstream_path(path: &str) -> &str {
    path.strip_prefix("/v1").unwrap_or(path)
}

pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
) -> Result<DeviceInit, ChannelError> {
    let oauth_host = oauth_host(settings, None).trim_end_matches('/');
    let base_url = setting(settings, "base_url").unwrap_or(DEFAULT_BASE_URL);
    let device_id = crate::util::rand::uuid_v4();
    let url = format!("{oauth_host}/api/oauth/device_authorization");
    let response = form_post(
        client,
        &url,
        &[("client_id", CLIENT_ID)],
        &device_id,
        "Kimi device authorization",
    )
    .await?;
    let parsed: DeviceAuthorization = successful_json(response, "Kimi device authorization")?;
    require_non_empty(&parsed.device_code, "device_code")?;
    require_non_empty(&parsed.user_code, "user_code")?;
    require_non_empty(
        &parsed.verification_uri_complete,
        "verification_uri_complete",
    )?;
    let state = PendingDevice {
        device_code: parsed.device_code,
        device_id,
        oauth_host: oauth_host.to_owned(),
        base_url: base_url.to_owned(),
    };
    Ok(DeviceInit {
        device_code: encode_state(&state)?,
        user_code: parsed.user_code,
        verification_url: parsed.verification_uri_complete,
        interval_secs: parsed.interval.max(1),
    })
}

pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
    encoded_state: &str,
) -> Result<DevicePoll, ChannelError> {
    let state = decode_state(encoded_state)?;
    let configured_host = oauth_host(settings, None);
    let host = if setting(settings, "oauth_host").is_some() {
        configured_host.trim_end_matches('/')
    } else {
        state.oauth_host.trim_end_matches('/')
    };
    let url = format!("{host}/api/oauth/token");
    let response = form_post(
        client,
        &url,
        &[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", CLIENT_ID),
            ("device_code", &state.device_code),
        ],
        &state.device_id,
        "Kimi device token",
    )
    .await?;
    let status = response.status();
    let payload: TokenPayload = serde_json::from_slice(response.body()).map_err(|error| {
        ChannelError::Build(format!("Kimi device token response parse: {error}"))
    })?;
    if status.is_success()
        && payload
            .access_token
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    {
        return token_secret(payload, state).map(DevicePoll::Ready);
    }
    match payload.error.as_deref() {
        Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
        Some("expired_token") | Some("access_denied") => Ok(DevicePoll::Denied),
        Some(error) => Err(ChannelError::Build(format!(
            "Kimi device token error: {error}{}",
            payload
                .error_description
                .as_deref()
                .map(|detail| format!(": {detail}"))
                .unwrap_or_default()
        ))),
        None => Err(ChannelError::Build(format!(
            "Kimi device token endpoint returned {status} without a token or OAuth error"
        ))),
    }
}

fn token_secret(payload: TokenPayload, state: PendingDevice) -> Result<Value, ChannelError> {
    let access_token = payload
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Build("Kimi token response missing access_token".into()))?;
    let refresh_token = payload
        .refresh_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Build("Kimi token response missing refresh_token".into()))?;
    let expires_in = payload
        .expires_in
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| ChannelError::Build("Kimi token response missing expires_in".into()))?;
    let expires_at_ms = now_ms().saturating_add(expires_in as i64 * 1000);
    Ok(json!({
        "type": "kimi",
        "auth_kind": "oauth",
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_at_ms": expires_at_ms,
        "device_id": state.device_id,
        "base_url": state.base_url,
        "oauth_host": state.oauth_host,
    }))
}

pub(super) fn needs_refresh(secret: &Value) -> bool {
    if field(secret, "access_token").is_none() {
        return true;
    }
    match secret.get("expires_at_ms").and_then(Value::as_i64) {
        Some(expires_at) => now_ms() > expires_at.saturating_sub(EXPIRY_SKEW_MS),
        None => false,
    }
}

pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
    settings: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = field(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing Kimi refresh_token".into()))?;
    let device_id = field(secret, "device_id")
        .ok_or_else(|| ChannelError::InvalidCredential("missing Kimi device_id".into()))?;
    let host = oauth_host(settings, Some(secret)).trim_end_matches('/');
    let url = format!("{host}/api/oauth/token");
    let response = form_post(
        client,
        &url,
        &[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
        device_id,
        "Kimi token refresh",
    )
    .await?;
    let payload: TokenPayload = successful_json(response, "Kimi token refresh")?;
    let access_token = payload
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Build("Kimi refresh response missing access_token".into()))?;
    let refresh_token = payload
        .refresh_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Build("Kimi refresh response missing refresh_token".into()))?;
    let expires_in = payload
        .expires_in
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| ChannelError::Build("Kimi refresh response missing expires_in".into()))?;

    let mut out = secret.clone();
    let object = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("Kimi secret is not an object".into()))?;
    object.insert("access_token".into(), Value::String(access_token));
    object.insert("refresh_token".into(), Value::String(refresh_token));
    object.insert(
        "expires_at_ms".into(),
        Value::Number(now_ms().saturating_add(expires_in as i64 * 1000).into()),
    );
    object.insert("oauth_host".into(), Value::String(host.to_owned()));
    object
        .entry("base_url")
        .or_insert_with(|| Value::String(DEFAULT_BASE_URL.into()));
    object
        .entry("auth_kind")
        .or_insert_with(|| Value::String("oauth".into()));
    Ok(out)
}

pub(super) fn apply(req: &mut Request<Bytes>, secret: &Value) -> Result<(), ChannelError> {
    let access_token = field(secret, "access_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing Kimi access_token".into()))?;
    let device_id = field(secret, "device_id")
        .ok_or_else(|| ChannelError::InvalidCredential("missing Kimi device_id".into()))?;
    let authorization = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|error| ChannelError::InvalidCredential(format!("bad Kimi token: {error}")))?;
    let is_get = req.method() == http::Method::GET;
    let headers = req.headers_mut();
    headers.insert(AUTHORIZATION, authorization);
    if !headers.contains_key(CONTENT_TYPE) && !is_get {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    apply_identity(headers, device_id)
}

fn apply_identity(headers: &mut HeaderMap, device_id: &str) -> Result<(), ChannelError> {
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    insert_static(headers, "x-msh-platform", PLATFORM);
    insert_static(headers, "x-msh-version", CLI_VERSION);
    insert_ascii(headers, "x-msh-device-name", device_name())?;
    insert_ascii(headers, "x-msh-device-model", device_model())?;
    insert_ascii(headers, "x-msh-os-version", os_version())?;
    insert_ascii(headers, "x-msh-device-id", device_id.to_owned())?;
    Ok(())
}

async fn form_post(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    form: &[(&str, &str)],
    device_id: &str,
    what: &str,
) -> Result<http::Response<Bytes>, ChannelError> {
    let body = form
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                crate::channel::oauth::percent_encode(key),
                crate::channel::oauth::percent_encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    let mut request = Request::post(url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Build(format!("{what} request build: {error}")))?;
    apply_identity(request.headers_mut(), device_id)?;
    client
        .send(request)
        .await
        .map_err(|error| ChannelError::Build(format!("{what} request failed: {error}")))
}

fn successful_json<T: serde::de::DeserializeOwned>(
    response: http::Response<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    if !response.status().is_success() {
        let snippet: String = String::from_utf8_lossy(response.body())
            .chars()
            .take(256)
            .collect();
        return Err(ChannelError::Build(format!(
            "{what} endpoint returned {}: {snippet}",
            response.status()
        )));
    }
    serde_json::from_slice(response.body())
        .map_err(|error| ChannelError::Build(format!("{what} response parse: {error}")))
}

fn encode_state(state: &PendingDevice) -> Result<String, ChannelError> {
    serde_json::to_vec(state)
        .map(|json| format!("{DEVICE_STATE_PREFIX}{}", B64URL.encode(json)))
        .map_err(|error| ChannelError::Build(format!("Kimi device state encode: {error}")))
}

fn decode_state(encoded: &str) -> Result<PendingDevice, ChannelError> {
    let payload = encoded.strip_prefix(DEVICE_STATE_PREFIX).ok_or_else(|| {
        ChannelError::Build("Kimi device state is missing its channel prefix".into())
    })?;
    let bytes = B64URL
        .decode(payload)
        .map_err(|error| ChannelError::Build(format!("Kimi device state decode: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| ChannelError::Build(format!("Kimi device state parse: {error}")))
}

fn require_non_empty(value: &str, field: &str) -> Result<(), ChannelError> {
    if value.is_empty() {
        Err(ChannelError::Build(format!(
            "Kimi device authorization response missing {field}"
        )))
    } else {
        Ok(())
    }
}

fn setting<'a>(settings: &'a Value, key: &str) -> Option<&'a str> {
    field(settings, key)
}

pub(super) fn field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn insert_static(headers: &mut HeaderMap, name: &'static str, value: &'static str) {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_static(value),
    );
}

fn insert_ascii(
    headers: &mut HeaderMap,
    name: &'static str,
    value: String,
) -> Result<(), ChannelError> {
    let cleaned: String = value
        .chars()
        .filter(|character| character.is_ascii() && !character.is_ascii_control())
        .collect::<String>()
        .trim()
        .to_owned();
    let value = if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    };
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(&value)
            .map_err(|error| ChannelError::Build(format!("bad Kimi {name}: {error}")))?,
    );
    Ok(())
}

fn device_name() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".into())
}

fn os_version() -> String {
    std::env::var("KERNEL_RELEASE").unwrap_or_else(|_| std::env::consts::OS.into())
}

fn device_model() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

fn now_ms() -> i64 {
    crate::util::time::unix_now().saturating_mul(1000)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn device_state_round_trip_preserves_identity_and_endpoints() {
        let state = PendingDevice {
            device_code: "device-code".into(),
            device_id: "device-id".into(),
            oauth_host: "https://auth.kimi.com".into(),
            base_url: DEFAULT_BASE_URL.into(),
        };
        let encoded = encode_state(&state).unwrap();
        assert!(!encoded.contains("device-code"));
        let decoded = decode_state(&encoded).unwrap();
        assert_eq!(decoded.device_code, "device-code");
        assert_eq!(decoded.device_id, "device-id");
        assert_eq!(decoded.oauth_host, "https://auth.kimi.com");
        assert_eq!(decoded.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn runtime_request_requires_device_identity() {
        let mut request = Request::post("https://api.kimi.com/coding/v1/chat/completions")
            .body(Bytes::new())
            .unwrap();
        assert!(apply(&mut request, &json!({"access_token": "token"})).is_err());
    }
}
