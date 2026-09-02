use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use serde_json::Value;

pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "access_token").ok_or_else(|| ChannelError::Secret("access_token missing".into()))
}

pub(super) fn project_id(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "project_id").ok_or_else(|| ChannelError::Secret("project_id missing".into()))
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    crate::shared::google_oauth::refresh_due(secret)
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    settings: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    crate::shared::google_oauth::refresh(
        secret,
        settings,
        http,
        &super::profile::PROFILE,
        "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com",
        "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",
    )
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
