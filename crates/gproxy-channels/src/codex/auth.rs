use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub(super) const VERSION: &str = "0.147.0";
pub(super) const ORIGINATOR: &str = "codex_exec";
pub(super) const USER_AGENT: &str =
    "codex_exec/0.147.0 (Debian 13.0.0; x86_64) xterm-256color (codex_exec; 0.147.0)";

const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const EXPIRY_SKEW_SECONDS: i64 = 60;

pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    secret_string(secret, "access_token")
        .ok_or_else(|| ChannelError::Secret("access_token missing".into()))
}

pub(super) fn account_id(secret: &Value) -> Option<&str> {
    secret_string(secret, "account_id")
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
        let body = crate::shared::http::form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLIENT_ID),
        ]);
        let mut request = http::Request::post(TOKEN_URL)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(http::header::ACCEPT, "application/json")
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

pub(super) fn session_id(secret: &Value, headers: &http::HeaderMap) -> String {
    if let Some(value) = headers
        .get("session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return value.to_owned();
    }
    let window = unix_now_ms() / (20 * 60 * 1_000);
    let token = secret_string(secret, "access_token").unwrap_or_default();
    let digest = Sha256::digest(format!("codex-session:{token}:{window}"));
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
    secret: &Value,
    session_id: &str,
) -> Result<(), ChannelError> {
    insert(
        headers,
        AUTHORIZATION,
        &format!("Bearer {}", access_token(secret)?),
    )?;
    headers.insert(
        http::header::USER_AGENT,
        HeaderValue::from_static(USER_AGENT),
    );
    headers.insert(
        HeaderName::from_static("originator"),
        HeaderValue::from_static(ORIGINATOR),
    );
    insert(headers, HeaderName::from_static("session-id"), session_id)?;
    if !headers.contains_key("x-client-request-id") {
        insert(
            headers,
            HeaderName::from_static("x-client-request-id"),
            session_id,
        )?;
    }
    if let Some(account_id) = account_id(secret) {
        insert(
            headers,
            HeaderName::from_static("chatgpt-account-id"),
            account_id,
        )?;
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(())
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = secret_string(token, "access_token")
        .ok_or_else(|| ChannelError::Refresh("token response missing access_token".into()))?;
    let expires_in = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3_600)
        .max(0);
    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("secret must be a JSON object".into()))?;
    object.insert("access_token".into(), Value::String(access.into()));
    if let Some(refresh) = secret_string(token, "refresh_token") {
        object.insert("refresh_token".into(), Value::String(refresh.into()));
    }
    if let Some(id_token) = secret_string(token, "id_token") {
        object.insert("id_token".into(), Value::String(id_token.into()));
        if let Some(account_id) = account_id_from_jwt(id_token) {
            object.insert("account_id".into(), Value::String(account_id));
        }
    }
    object.insert(
        "expires_at_ms".into(),
        Value::from(unix_now_ms().saturating_add(expires_in.saturating_mul(1_000))),
    );
    Ok(output)
}

fn account_id_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64_url_decode(payload)?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn base64_url_decode(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes().filter(|byte| *byte != b'=') {
        accumulator = (accumulator << 6) | u32::from(base64_value(byte)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits).saturating_sub(1);
        }
    }
    Some(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn secret_string<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
