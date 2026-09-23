use base64::Engine as _;
use gproxy_channel_api::ChannelError;
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;

pub(super) fn apply(headers: &mut http::HeaderMap, secret: &Value) -> Result<(), ChannelError> {
    let bearer = bearer(secret)?;
    insert(headers, AUTHORIZATION, &format!("Bearer {bearer}"))?;
    for (name, value) in [
        ("http-referer", "https://cline.bot"),
        ("x-title", "Cline"),
        ("x-client-type", "cline-sdk"),
    ] {
        insert(headers, HeaderName::from_static(name), value)?;
    }
    Ok(())
}

pub(super) fn bearer(secret: &Value) -> Result<String, ChannelError> {
    if let Some(token) = field(secret, "access_token") {
        return Ok(login_bearer(token));
    }
    let key = field(secret, "api_key")
        .ok_or_else(|| ChannelError::Secret("access_token or api_key missing".into()))?;
    Ok(if is_login_token(key) {
        login_bearer(key)
    } else {
        key.into()
    })
}

// Older releases copied the login token into api_key during login and refresh.
pub(super) fn api_key(secret: &Value) -> Option<&str> {
    field(secret, "api_key")
        .filter(|key| Some(*key) != field(secret, "access_token") && !is_login_token(key))
}

fn login_bearer(token: &str) -> String {
    if token.to_ascii_lowercase().starts_with("workos:") {
        token.into()
    } else {
        format!("workos:{token}")
    }
}

fn is_login_token(token: &str) -> bool {
    if token.to_ascii_lowercase().starts_with("workos:") {
        return true;
    }
    let parts: Vec<_> = token.split('.').collect();
    parts.len() == 3
        && base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|header| header.get("alg").and_then(Value::as_str).is_some())
}

pub(super) fn token_expiry(token: &str) -> Option<i64> {
    let token = token.strip_prefix("workos:").unwrap_or(token);
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get("exp")?
        .as_i64()
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
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
            .map_err(|error| ChannelError::Prepare(format!("Cline header is invalid: {error}")))?,
    );
    Ok(())
}
