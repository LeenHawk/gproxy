//! Grok Build auth — xAI device-code/API-key bearer auth against the
//! OpenAI-like `https://api.x.ai/v1` Responses API.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use bytes::Bytes;
use http::Request;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::channel::ChannelError;
use crate::channel::oauth;
use crate::http::client::UpstreamClient;

pub(super) const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const DEVICE_CODE_URL: &str = "https://auth.x.ai/oauth2/device/code";
const OAUTH_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const OAUTH_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access conversations:read conversations:write";
const EXPIRY_SKEW_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcceptMode {
    Json,
    EventStream,
    Unset,
}

fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(
                char::from_digit((b >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit((b & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    out
}

fn secret_str<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    #[serde(default)]
    verification_uri: Option<String>,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    id_token: Option<String>,
    error: Option<String>,
}

fn default_interval() -> u64 {
    5
}

pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
) -> Result<crate::channel::DeviceInit, ChannelError> {
    let form = [("client_id", OAUTH_CLIENT_ID), ("scope", OAUTH_SCOPE)];
    let parsed: DeviceCodeResponse =
        form_post_json(client, DEVICE_CODE_URL, &form, "xai device code").await?;
    let verification_url = parsed
        .verification_uri_complete
        .or(parsed.verification_uri)
        .ok_or_else(|| {
            ChannelError::Build("xai device response missing verification_uri".into())
        })?;
    Ok(crate::channel::DeviceInit {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_url,
        interval_secs: parsed.interval,
    })
}

pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    device_code: &str,
) -> Result<crate::channel::DevicePoll, ChannelError> {
    use crate::channel::DevicePoll;

    let form = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", OAUTH_CLIENT_ID),
        ("device_code", device_code),
    ];
    let parsed: DeviceTokenResponse =
        form_post_json_allow_status(client, TOKEN_URL, &form, "xai device token").await?;

    if parsed
        .access_token
        .as_deref()
        .is_some_and(|s| !s.is_empty())
    {
        return token_secret(oauth::TokenResponse {
            access_token: parsed.access_token,
            refresh_token: parsed.refresh_token,
            expires_in: parsed.expires_in,
            id_token: parsed.id_token,
        })
        .map(DevicePoll::Ready);
    }

    match parsed.error.as_deref() {
        Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
        Some("access_denied") | Some("expired_token") => Ok(DevicePoll::Denied),
        Some(other) => Err(ChannelError::Build(format!(
            "xai device poll error: {other}"
        ))),
        None => Err(ChannelError::Build(
            "xai device poll: neither access_token nor error".into(),
        )),
    }
}

fn token_secret(resp: oauth::TokenResponse) -> Result<Value, ChannelError> {
    let access_token = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("xai token response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut secret = json!({
        "type": "xai",
        "auth_kind": "oauth",
        "access_token": access_token,
        "expires_at_ms": expires_at_ms,
        "base_url": DEFAULT_BASE_URL,
        "token_endpoint": TOKEN_URL,
    });
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        secret["refresh_token"] = Value::String(rt);
    }
    if let Some(id_token) = resp.id_token.filter(|s| !s.is_empty()) {
        if let Some(email) = jwt_claim(&id_token, "email") {
            secret["user_email"] = Value::String(email);
        }
        if let Some(sub) = jwt_claim(&id_token, "sub") {
            secret["sub"] = Value::String(sub);
        }
        secret["id_token"] = Value::String(id_token);
    }
    Ok(secret)
}

async fn form_post_json<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    form: &[(&str, &str)],
    what: &str,
) -> Result<T, ChannelError> {
    let resp = form_post(client, url, form, what).await?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "{what} endpoint {}: {snippet}",
            parts.status
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}

async fn form_post_json_allow_status<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    form: &[(&str, &str)],
    what: &str,
) -> Result<T, ChannelError> {
    let resp = form_post(client, url, form, what).await?;
    serde_json::from_slice(resp.body())
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}

async fn form_post(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    form: &[(&str, &str)],
    what: &str,
) -> Result<http::Response<Bytes>, ChannelError> {
    let body = form
        .iter()
        .map(|(key, value)| format!("{}={}", pct(key), pct(value)))
        .collect::<Vec<_>>()
        .join("&");
    let req = Request::post(url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|e| ChannelError::Build(format!("{what} request build: {e}")))?;
    client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("{what} request failed: {e}")))
}

pub(super) fn bearer_token(secret: &Value) -> Result<&str, ChannelError> {
    secret_str(secret, "access_token")
        .or_else(|| secret_str(secret, "api_key"))
        .ok_or_else(|| ChannelError::InvalidCredential("missing access_token or api_key".into()))
}

pub(super) fn base_url<'a>(settings: &'a Value, secret: &'a Value) -> &'a str {
    settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| secret_str(secret, "base_url"))
        .unwrap_or(DEFAULT_BASE_URL)
}

pub(super) fn upstream_path(base_url: &str, path: &str) -> String {
    if base_url.trim_end_matches('/').ends_with("/v1") {
        path.strip_prefix("/v1").unwrap_or(path).to_owned()
    } else {
        path.to_owned()
    }
}

pub(super) fn needs_refresh(secret: &Value) -> bool {
    if secret_str(secret, "api_key").is_some() && secret_str(secret, "refresh_token").is_none() {
        return false;
    }
    if secret_str(secret, "access_token").is_none() {
        return true;
    }
    let expires_at_ms = secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if expires_at_ms == 0 {
        return false;
    }
    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    now_ms > expires_at_ms - EXPIRY_SKEW_MS
}

pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = secret_str(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;
    let token_url = secret_str(secret, "token_endpoint").unwrap_or(TOKEN_URL);
    validate_xai_endpoint(token_url, "token_endpoint")?;
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", OAUTH_CLIENT_ID),
        ("refresh_token", refresh_token),
    ];
    let resp = oauth::token_post(client, token_url, &form, &[]).await?;

    let new_access = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("xai refresh response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut out = secret.clone();
    let obj = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(new_access));
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        obj.insert("refresh_token".into(), Value::String(rt));
    }
    if let Some(id_token) = resp.id_token.filter(|s| !s.is_empty()) {
        if let Some(email) = jwt_claim(&id_token, "email") {
            obj.insert("user_email".into(), Value::String(email));
        }
        if let Some(sub) = jwt_claim(&id_token, "sub") {
            obj.insert("sub".into(), Value::String(sub));
        }
        obj.insert("id_token".into(), Value::String(id_token));
    }
    obj.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    obj.entry("auth_kind")
        .or_insert_with(|| Value::String("oauth".into()));
    obj.entry("type")
        .or_insert_with(|| Value::String("xai".into()));
    obj.entry("base_url")
        .or_insert_with(|| Value::String(DEFAULT_BASE_URL.into()));
    obj.entry("token_endpoint")
        .or_insert_with(|| Value::String(token_url.into()));
    Ok(out)
}

pub(super) fn session_id_from_body(body: &Bytes) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    value
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn apply(
    req: &mut Request<Bytes>,
    bearer_token: &str,
    accept: AcceptMode,
    session_id: Option<&str>,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {bearer_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad bearer token: {e}")))?;
    let headers = req.headers_mut();
    headers.insert(AUTHORIZATION, bearer);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    match accept {
        AcceptMode::Json => {
            headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        }
        AcceptMode::EventStream => {
            headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        }
        AcceptMode::Unset => {
            headers.remove(ACCEPT);
        }
    }
    if let Some(session_id) = session_id {
        let value = HeaderValue::from_str(session_id)
            .map_err(|e| ChannelError::Build(format!("bad x-grok-conv-id: {e}")))?;
        headers.insert(HeaderName::from_static("x-grok-conv-id"), value);
    }
    Ok(())
}

fn validate_xai_endpoint(raw_url: &str, field: &str) -> Result<(), ChannelError> {
    let uri: http::Uri = raw_url
        .parse()
        .map_err(|e| ChannelError::Build(format!("xai {field} is invalid: {e}")))?;
    if uri.scheme_str() != Some("https") {
        return Err(ChannelError::Build(format!("xai {field} must use https")));
    }
    let host = uri.host().unwrap_or_default().to_ascii_lowercase();
    if host != "x.ai" && !host.ends_with(".x.ai") {
        return Err(ChannelError::Build(format!(
            "xai {field} host {host:?} is not on x.ai"
        )));
    }
    Ok(())
}

fn jwt_claim(id_token: &str, claim: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = B64URL.decode(payload).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get(claim)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}
