use serde_json::Value;

pub fn provider_settings(channel: &str, settings: &Value) -> Result<Value, String> {
    let Some(tier) = legacy_tier(channel) else {
        if channel == "opencode" {
            validate_opencode(settings)?;
        }
        return Ok(settings.clone());
    };
    let mut object = settings
        .as_object()
        .cloned()
        .ok_or_else(|| format!("legacy channel `{channel}` settings must be an object"))?;
    object.insert("tier".into(), Value::String(tier.into()));
    Ok(Value::Object(object))
}

fn validate_opencode(settings: &Value) -> Result<(), String> {
    let object = settings
        .as_object()
        .ok_or_else(|| "opencode settings must be an object".to_owned())?;
    match object.get("tier") {
        None => Ok(()),
        Some(Value::String(tier)) if tier == "zen" || tier == "go" => Ok(()),
        Some(Value::String(tier)) => Err(format!("unknown opencode tier `{tier}`")),
        Some(_) => Err("opencode tier must be `zen` or `go`".into()),
    }
}

fn legacy_tier(channel: &str) -> Option<&'static str> {
    match channel {
        "opencodezen" => Some("zen"),
        "opencodego" => Some("go"),
        _ => None,
    }
}
