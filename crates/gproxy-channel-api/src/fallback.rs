#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaudeFallbackCapabilities {
    pub server_side: bool,
    pub credit: bool,
    pub recommended_model: Option<&'static str>,
}

pub fn has_fallback_credit(body: &serde_json::Value) -> bool {
    body.get("fallback_credit_token")
        .is_some_and(|value| !value.is_null())
}

pub fn claude_fallback_setting(settings: &serde_json::Value) -> Option<serde_json::Value> {
    match settings
        .get("claude_fallback_mode")
        .and_then(serde_json::Value::as_str)
    {
        Some("default") => Some(serde_json::json!("default")),
        Some("models") => settings.get("claude_fallback_models").cloned(),
        Some("off") => None,
        _ => settings.get("claude_fable_fallbacks").cloned(),
    }
}
