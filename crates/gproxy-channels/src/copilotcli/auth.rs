use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, AUTHORIZATION, HeaderValue};
use serde_json::Value;

const TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const DEFAULT_VSCODE_VERSION: &str = "1.95.3";
const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.43.0";
const USER_AGENT: &str = "copilot/1.0.61 (linux v24.16.0) term/unknown";
const API_VERSION: &str = "2025-04-01";
const EXPIRY_SKEW_SECONDS: i64 = 60;

pub(super) fn copilot_token(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "copilot_token")
        .ok_or_else(|| ChannelError::Secret("copilot_token missing after refresh".into()))
}

pub(super) fn account_base(secret: &Value) -> &'static str {
    match field(secret, "account_type") {
        Some("business") => "https://api.business.githubcopilot.com",
        Some("enterprise") => "https://api.enterprise.githubcopilot.com",
        _ => "https://api.githubcopilot.com",
    }
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    if field(secret, "copilot_token").is_none() {
        return Some(i64::MIN);
    }
    match secret.get("copilot_expires_at_ms").and_then(Value::as_i64) {
        Some(expiry) if expiry != 0 => Some(expiry / 1_000 - EXPIRY_SKEW_SECONDS),
        _ => Some(i64::MIN),
    }
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = refresh_request(secret);
    let request = match request {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            return Err(ChannelError::Refresh(format!(
                "Copilot token endpoint returned {}",
                response.status()
            )));
        }
        let token: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("Copilot token JSON: {error}")))?;
        rotate(secret, &token)
    })
}

fn refresh_request(secret: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    github_request(secret, TOKEN_URL)
}

/// GitHub-token request with the Copilot editor fingerprint — the long-lived
/// `token <github_token>` scheme, not the short-lived Copilot bearer.
pub(super) fn github_request(
    secret: &Value,
    url: &str,
) -> Result<http::Request<Bytes>, ChannelError> {
    let github = required(secret, "github_token")?;
    let vscode = field(secret, "vscode_version").unwrap_or(DEFAULT_VSCODE_VERSION);
    let mut request = http::Request::get(url)
        .header(AUTHORIZATION, format!("token {github}"))
        .header("editor-version", format!("vscode/{vscode}"))
        .header("editor-plugin-version", EDITOR_PLUGIN_VERSION)
        .header(http::header::USER_AGENT, USER_AGENT)
        .header("x-github-api-version", API_VERSION)
        .header(ACCEPT, "application/json")
        .body(Bytes::new())
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    request
        .extensions_mut()
        .insert(super::profile::CLIENT_PROFILE.clone());
    Ok(request)
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = required(token, "token")?;
    let expiry = token
        .get("expires_at")
        .and_then(Value::as_i64)
        .ok_or_else(|| ChannelError::Refresh("token response missing expires_at".into()))?;
    let mut output = secret.clone();
    let root = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("Copilot secret must be an object".into()))?;
    root.insert("copilot_token".into(), Value::String(access.into()));
    root.insert(
        "copilot_expires_at_ms".into(),
        Value::from(expiry.saturating_mul(1_000)),
    );
    Ok(output)
}

pub(super) fn insert_bearer(
    headers: &mut http::HeaderMap,
    token: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|error| ChannelError::Secret(format!("copilot_token is invalid: {error}")))?,
    );
    Ok(())
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    field(value, name).ok_or_else(|| ChannelError::Secret(format!("{name} missing")))
}
