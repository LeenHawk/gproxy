use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const TOKEN_URL: &str = "https://api.anthropic.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const OAUTH_SCOPE: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
pub(super) const OAUTH_BETA: &str = "oauth-2025-04-20";
pub(super) const CLI_USER_AGENT: &str = "claude-cli/2.1.112 (external, cli)";
const EXPIRY_SKEW_SECONDS: i64 = 30 * 60;

pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    secret_string(secret, "access_token")
        .ok_or_else(|| ChannelError::Secret("access_token missing".into()))
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    if secret_string(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    let expires_at_ms = secret.get("expires_at_ms")?.as_i64()?;
    (expires_at_ms != 0).then(|| expires_at_ms / 1000 - EXPIRY_SKEW_SECONDS)
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = (|| {
        let refresh_token = secret_string(secret, "refresh_token")
            .ok_or_else(|| ChannelError::Refresh("refresh_token missing".into()))?;
        let scope = refresh_scope(secret);
        let body = form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", refresh_token),
            ("scope", &scope),
        ]);
        let mut request = http::Request::post(TOKEN_URL)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(http::header::ACCEPT, "application/json, text/plain, */*")
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", OAUTH_BETA)
            .header(http::header::USER_AGENT, CLI_USER_AGENT)
            .body(Bytes::from(body))
            .map_err(|error| ChannelError::Refresh(error.to_string()))?;
        request
            .extensions_mut()
            .insert(super::profile::CLIENT_PROFILE);
        Ok(request)
    })();
    let request = match request {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            let snippet: String = String::from_utf8_lossy(response.body())
                .chars()
                .take(256)
                .collect();
            return Err(ChannelError::Refresh(format!(
                "token endpoint {}: {snippet}",
                response.status()
            )));
        }
        let token: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("invalid token response: {error}")))?;
        rotate(secret, &token)
    })
}

pub(super) fn device_id(secret: &Value) -> String {
    if let Some(device) = secret_string(secret, "device_id") {
        return device.to_owned();
    }
    let seed = secret_string(secret, "account_uuid")
        .or_else(|| secret_string(secret, "refresh_token"))
        .or_else(|| secret_string(secret, "access_token"))
        .unwrap_or_default();
    hex(Sha256::digest(format!("claudecode-device:{seed}").as_bytes()).as_slice())
}

pub(super) fn session_id(secret: &Value, headers: &http::HeaderMap) -> String {
    if let Some(explicit) = headers
        .get("x-claude-code-session-id")
        .or_else(|| headers.get("session_id"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return explicit.to_owned();
    }
    let window = unix_now_ms() / (20 * 60 * 1000);
    let digest = Sha256::digest(format!("claudecode-session:{}:{window}", device_id(secret)));
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    )
}

pub(super) fn apply_headers(
    headers: &mut http::HeaderMap,
    token: &str,
    session_id: &str,
) -> Result<(), ChannelError> {
    insert(headers, AUTHORIZATION, &format!("Bearer {token}"))?;
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static("2023-06-01"),
    );
    let client_beta = headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok());
    insert(
        headers,
        HeaderName::from_static("anthropic-beta"),
        &merge_beta(client_beta),
    )?;
    for (name, value) in [
        ("anthropic-dangerous-direct-browser-access", "true"),
        ("x-app", "cli"),
        ("x-stainless-retry-count", "0"),
        ("x-stainless-timeout", "86400"),
        ("x-stainless-lang", "js"),
        ("x-stainless-package-version", "0.81.0"),
        ("x-stainless-runtime", "node"),
        ("x-stainless-runtime-version", "v22.20.0"),
    ] {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    insert(
        headers,
        HeaderName::from_static("x-claude-code-session-id"),
        session_id,
    )?;
    insert(
        headers,
        HeaderName::from_static("x-stainless-os"),
        stainless_os(),
    )?;
    insert(
        headers,
        HeaderName::from_static("x-stainless-arch"),
        stainless_arch(),
    )?;
    headers.insert(
        http::header::USER_AGENT,
        HeaderValue::from_static(CLI_USER_AGENT),
    );
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(http::header::ACCEPT_LANGUAGE, HeaderValue::from_static("*"));
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("cors"),
    );
    headers.insert(
        http::header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate"),
    );
    Ok(())
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = secret_string(token, "access_token")
        .ok_or_else(|| ChannelError::Refresh("token response missing access_token".into()))?;
    let expires_in = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600)
        .max(0);
    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("secret must be a JSON object".into()))?;
    object.insert("access_token".into(), Value::String(access.into()));
    if let Some(refresh) = secret_string(token, "refresh_token") {
        object.insert("refresh_token".into(), Value::String(refresh.into()));
    }
    if let Some(scope) = secret_string(token, "scope") {
        object.insert(
            "scopes".into(),
            Value::Array(
                scope
                    .split_whitespace()
                    .map(|value| Value::String(value.into()))
                    .collect(),
            ),
        );
    }
    object.insert(
        "expires_at_ms".into(),
        Value::from(unix_now_ms().saturating_add(expires_in.saturating_mul(1000))),
    );
    if !object.contains_key("device_id") {
        object.insert("device_id".into(), Value::String(device_id(secret)));
    }
    Ok(output)
}

fn refresh_scope(secret: &Value) -> String {
    let mut scopes = OAUTH_SCOPE.split_whitespace().collect::<Vec<_>>();
    let stored = secret
        .get("scopes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    for scope in stored {
        if matches!(scope, "user:projects:read" | "user:projects:write") && !scopes.contains(&scope)
        {
            scopes.push(scope);
        }
    }
    scopes.join(" ")
}

fn merge_beta(client: Option<&str>) -> String {
    let mut values = vec![OAUTH_BETA];
    for value in client
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !values.contains(&value) {
            values.push(value);
        }
    }
    values.join(",")
}

fn insert(
    headers: &mut http::HeaderMap,
    name: HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        name,
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Prepare(format!("invalid header: {error}")))?,
    );
    Ok(())
}

fn secret_string<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{}={}", percent(name), percent(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String succeeds");
    }
    output
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_millis()
        .try_into()
        .expect("Unix milliseconds fit in i64")
}

#[cfg(target_arch = "wasm32")]
fn stainless_os() -> &'static str {
    "Linux"
}

#[cfg(not(target_arch = "wasm32"))]
fn stainless_os() -> &'static str {
    match std::env::consts::OS {
        "ios" => "iOS",
        "android" => "Android",
        "macos" => "MacOS",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "linux" => "Linux",
        _ => "Unknown",
    }
}

#[cfg(target_arch = "wasm32")]
fn stainless_arch() -> &'static str {
    "x64"
}

#[cfg(not(target_arch = "wasm32"))]
fn stainless_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x32",
        "arm" => "arm",
        _ => "unknown",
    }
}
