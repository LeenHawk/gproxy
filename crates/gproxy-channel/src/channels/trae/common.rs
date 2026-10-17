use crate::channel::ChannelError;
use http::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

pub(super) fn invalid(message: impl std::fmt::Display) -> ChannelError {
    ChannelError::InvalidResponse(format!("Trae: {message}"))
}

pub(super) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub(super) fn random_hex<const N: usize>() -> Result<String, ChannelError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| ChannelError::InvalidConfig("Trae: randomness unavailable".into()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) fn insert(
    headers: &mut HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)?,
    );
    Ok(())
}

pub(super) fn json(body: &[u8]) -> Result<Value, ChannelError> {
    serde_json::from_slice(body).map_err(invalid)
}

pub(super) fn check(value: &Value) -> Result<(), ChannelError> {
    if let Some(error) = value
        .pointer("/ResponseMetadata/Error")
        .filter(|v| !v.is_null())
    {
        return Err(invalid(format!("{} ({})", error["Message"], error["Code"])));
    }
    if value.get("code").is_some_and(|v| v.as_i64() != Some(0)) {
        return Err(invalid(format!("{} ({})", value["message"], value["code"])));
    }
    Ok(())
}
