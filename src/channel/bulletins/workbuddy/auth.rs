use bytes::Bytes;
use http::Request;
use http::header::{AUTHORIZATION, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::channel::ChannelError;

pub(super) const DEFAULT_BASE_URL: &str = "https://copilot.tencent.com";

pub(super) fn base_url(settings: &Value) -> &str {
    settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
}

pub(super) fn field<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn apply(req: &mut Request<Bytes>, secret: &Value) -> Result<(), ChannelError> {
    let token = field(secret, "access_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing access_token".into()))?;
    let user_id = field(secret, "user_id")
        .ok_or_else(|| ChannelError::InvalidCredential("missing user_id".into()))?;

    insert(req, AUTHORIZATION, &format!("Bearer {token}"))?;
    insert_named(req, "x-user-id", user_id)?;
    if let Some(enterprise_id) = field(secret, "enterprise_id") {
        insert_named(req, "x-enterprise-id", enterprise_id)?;
        insert_named(req, "x-tenant-id", enterprise_id)?;
    }
    if let Some(department) = field(secret, "department_full_name") {
        insert_named(req, "x-department-info", department)?;
    }
    if let Some(domain) = field(secret, "domain") {
        insert_named(req, "x-domain", domain)?;
    }

    let request_id = crate::util::rand::uuid_v4().replace('-', "");
    let conversation_id = crate::util::rand::uuid_v4().replace('-', "");
    for (name, value) in [
        ("x-request-id", request_id.as_str()),
        ("x-conversation-message-id", request_id.as_str()),
        ("x-conversation-request-id", request_id.as_str()),
        ("x-conversation-id", conversation_id.as_str()),
        ("x-agent-intent", "craft"),
        ("x-product", "SaaS"),
        ("x-ide-type", "CLI"),
        ("x-ide-name", "CLI"),
        ("x-ide-version", "4.22.16"),
    ] {
        insert_named(req, name, value)?;
    }
    req.headers_mut()
        .insert(USER_AGENT, HeaderValue::from_static("WorkBuddy/4.22.16"));
    Ok(())
}

pub(super) fn apply_refresh(req: &mut Request<Bytes>, secret: &Value) -> Result<(), ChannelError> {
    let refresh = field(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;
    insert_named(req, "x-refresh-token", refresh)?;
    insert_named(req, "x-auth-refresh-source", "plugin")?;
    if let Some(domain) = field(secret, "domain") {
        insert_named(req, "x-domain", domain)?;
    }
    insert_named(req, "x-product", "SaaS")
}

pub(super) fn insert_named(
    req: &mut Request<Bytes>,
    name: &'static str,
    value: &str,
) -> Result<(), ChannelError> {
    insert(req, HeaderName::from_static(name), value)
}

fn insert(req: &mut Request<Bytes>, name: HeaderName, value: &str) -> Result<(), ChannelError> {
    let value = HeaderValue::from_str(value)
        .map_err(|error| ChannelError::InvalidCredential(format!("bad header value: {error}")))?;
    req.headers_mut().insert(name, value);
    Ok(())
}
