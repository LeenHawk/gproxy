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
        return Ok(if token.to_ascii_lowercase().starts_with("workos:") {
            token.into()
        } else {
            format!("workos:{token}")
        });
    }
    field(secret, "api_key")
        .map(str::to_owned)
        .ok_or_else(|| ChannelError::Secret("access_token or api_key missing".into()))
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
