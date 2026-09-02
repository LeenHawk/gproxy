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
        "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com",
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
