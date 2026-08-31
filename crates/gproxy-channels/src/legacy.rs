use serde_json::Value;

pub fn provider_settings(channel: &str, settings: &Value) -> Result<Value, String> {
    let output = if let Some(tier) = legacy_tier(channel) {
        let mut object = settings
            .as_object()
            .cloned()
            .ok_or_else(|| format!("legacy channel `{channel}` settings must be an object"))?;
        object.insert("tier".into(), Value::String(tier.into()));
        Value::Object(object)
    } else {
        if channel == "opencode" {
            validate_opencode(settings)?;
        }
        settings.clone()
    };
    gproxy_channel_api::TrafficPolicyConfig::configured(&output)?;
    Ok(output)
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
