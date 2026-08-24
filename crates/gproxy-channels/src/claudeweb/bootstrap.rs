use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn merge(secret: &Value, body: &[u8]) -> Result<Value, ChannelError> {
    let bootstrap = serde_json::Deserializer::from_slice(body)
        .into_iter::<Value>()
        .filter_map(Result::ok)
        .find(|value| value.get("account").is_some())
        .ok_or_else(|| ChannelError::Refresh("bootstrap account JSON missing".into()))?;
    let account = bootstrap
        .get("account")
        .and_then(Value::as_object)
        .ok_or_else(|| ChannelError::Refresh("Claude session is not logged in".into()))?;
    let organization = account
        .get("memberships")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|membership| membership.get("organization"))
        .filter(|organization| has_capability(organization, "chat"))
        .max_by_key(|organization| {
            organization
                .get("capabilities")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        })
        .ok_or_else(|| ChannelError::Refresh("bootstrap has no chat organization".into()))?;
    let uuid = organization
        .get("uuid")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Refresh("chat organization UUID missing".into()))?;
    let mut output = secret
        .as_object()
        .cloned()
        .ok_or_else(|| ChannelError::Refresh("secret must be an object".into()))?;
    for field in [
        "account_uuid",
        "capabilities",
        "active_flags",
        "claude_ai_bootstrap_models_config",
        "user_email",
        "rate_limit_tier",
    ] {
        output.remove(field);
    }
    output.insert("account_uuid".into(), Value::String(uuid.into()));
    for (source, target) in [
        ("capabilities", "capabilities"),
        ("active_flags", "active_flags"),
        (
            "claude_ai_bootstrap_models_config",
            "claude_ai_bootstrap_models_config",
        ),
    ] {
        if let Some(value) = organization.get(source) {
            output.insert(target.into(), value.clone());
        }
    }
    if let Some(email) = account.get("email_address").and_then(Value::as_str) {
        output.insert("user_email".into(), Value::String(email.into()));
    }
    if let Some(tier) = organization
        .get("rate_limit_tier")
        .or_else(|| organization.get("rateLimitTier"))
    {
        output.insert("rate_limit_tier".into(), tier.clone());
    }
    output.insert("validated_at_ms".into(), Value::from(unix_ms()?));
    Ok(Value::Object(output))
}

fn has_capability(organization: &Value, name: &str) -> bool {
    organization
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(name)))
}

fn unix_ms() -> Result<i64, ChannelError> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ChannelError::Refresh("system clock is before Unix epoch".into()))?
        .as_millis();
    i64::try_from(millis).map_err(|_| ChannelError::Refresh("Unix milliseconds overflow".into()))
}
