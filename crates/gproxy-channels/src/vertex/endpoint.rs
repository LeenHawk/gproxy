use gproxy_channel_api::ChannelError;
use serde_json::Value;

const DEFAULT_LOCATION: &str = "us-central1";

pub(super) fn project_id(secret: &Value) -> Result<&str, ChannelError> {
    let project = string(secret, "project_id")
        .ok_or_else(|| ChannelError::Secret("project_id missing".into()))?;
    validate_segment(project, "project_id")?;
    Ok(project)
}

pub(super) fn location<'a>(
    settings: &'a Value,
    secret: &'a Value,
) -> Result<&'a str, ChannelError> {
    let location = string(settings, "location")
        .or_else(|| string(secret, "location"))
        .unwrap_or(DEFAULT_LOCATION);
    validate_segment(location, "location")?;
    Ok(location)
}

pub(super) fn default_base(settings: &Value, secret: &Value) -> Result<String, ChannelError> {
    if let Some(base) = string(settings, "base_url") {
        return Ok(base.to_owned());
    }
    let location = location(settings, secret)?;
    if location == "global" {
        Ok("https://aiplatform.googleapis.com".into())
    } else {
        Ok(format!("https://{location}-aiplatform.googleapis.com"))
    }
}

fn validate_segment(value: &str, name: &str) -> Result<(), ChannelError> {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(ChannelError::Secret(format!(
            "{name} contains invalid characters"
        )))
    }
}

fn string<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
