use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn region(settings: &Value) -> Result<&str, ChannelError> {
    let region = field(settings, "region").unwrap_or("us-east-1");
    validate_region(region)?;
    Ok(region)
}

pub(super) fn runtime(settings: &Value) -> Result<String, ChannelError> {
    if let Some(url) = field(settings, "base_url") {
        return Ok(url.into());
    }
    Ok(format!("https://runtime.{}.kiro.dev", region(settings)?))
}

pub(super) fn management(settings: &Value) -> Result<String, ChannelError> {
    if let Some(url) = field(settings, "base_url") {
        return Ok(url.into());
    }
    Ok(format!("https://management.{}.kiro.dev", region(settings)?))
}

pub(super) fn exact(settings: &Value, name: &str, model: &str) -> Option<String> {
    let url = settings.get("endpoints")?.get(name)?.as_str()?.trim();
    (!url.is_empty()).then(|| {
        url.replace(
            "{model}",
            &crate::shared::http::encode_component(model.trim()),
        )
    })
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn validate_region(region: &str) -> Result<(), ChannelError> {
    if region
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(ChannelError::Prepare(
            "Kiro region contains invalid characters".into(),
        ))
    }
}
