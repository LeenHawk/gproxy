use gproxy_channel_api::ChannelError;
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://copilot.tencent.com";

pub(super) fn base_url(settings: &Value) -> &str {
    field(settings, "base_url").unwrap_or(DEFAULT_BASE_URL)
}

pub(super) fn apply(headers: &mut http::HeaderMap, secret: &Value) -> Result<(), ChannelError> {
    insert(
        headers,
        AUTHORIZATION,
        &format!("Bearer {}", required(secret, "access_token")?),
    )?;
    insert(
        headers,
        HeaderName::from_static("x-user-id"),
        required(secret, "user_id")?,
    )?;
    for (field_name, headers_names) in [
        ("enterprise_id", &["x-enterprise-id", "x-tenant-id"][..]),
        ("department_full_name", &["x-department-info"][..]),
        ("domain", &["x-domain"][..]),
    ] {
        if let Some(value) = field(secret, field_name) {
            for name in headers_names {
                insert(headers, HeaderName::from_static(name), value)?;
            }
        }
    }
    Ok(())
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    field(value, name).ok_or_else(|| ChannelError::Secret(format!("{name} missing")))
}

pub(super) fn insert(
    headers: &mut http::HeaderMap,
    name: HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        name,
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Secret(format!("WorkBuddy credential: {error}")))?,
    );
    Ok(())
}
