//! Provider routing preferences, the service tier name and usage accounting.
//!
//! OpenRouter serves one model name from several physical providers and takes
//! a `provider` object in the request body to say which ones, in what order,
//! and whether it may fall back (`order`, `only`, `ignore`, `allow_fallbacks`,
//! `sort`, `require_parameters`, `data_collection`, `quantizations`,
//! `max_price`). The channel does not interpret the object: an operator's
//! preferences are forwarded as written, and a `provider` the client already
//! sent is never second-guessed.
//!
//! `usage.include` is the switch that makes OpenRouter report what it charged
//! (`usage.cost`); without it the reply has token counts only and the
//! exchange can be priced from local rates alone.

use super::config::OpenRouterConfig;
use crate::channel::ChannelError;
use serde_json::{Map, Value, json};

/// Apply every body-level preference. Returns whether anything changed, so a
/// body that needs nothing keeps its original bytes.
pub(super) fn apply(value: &mut Value, config: &OpenRouterConfig) -> Result<bool, ChannelError> {
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let Some(object) = value.as_object_mut() else {
        return Ok(false);
    };
    let mut changed = false;
    if config.normalize_service_tier
        && object.get("service_tier").and_then(Value::as_str) == Some("fast")
    {
        object.insert("service_tier".into(), Value::String("priority".into()));
        changed = true;
    }
    changed |= routing(object, config, model.as_deref())?;
    changed |= usage_accounting(object, config);
    Ok(changed)
}

/// The client's own `provider` wins; otherwise the model's override, then the
/// provider-level default.
fn routing(
    object: &mut Map<String, Value>,
    config: &OpenRouterConfig,
    model: Option<&str>,
) -> Result<bool, ChannelError> {
    if object.get("provider").is_some_and(|value| !value.is_null()) {
        return Ok(false);
    }
    let Some(preferences) = config.routing(model)? else {
        return Ok(false);
    };
    if preferences.is_empty() {
        return Ok(false);
    }
    object.insert("provider".into(), Value::Object(preferences.clone()));
    Ok(true)
}

/// `usage.include` is only added when the caller expressed no opinion.
fn usage_accounting(object: &mut Map<String, Value>, config: &OpenRouterConfig) -> bool {
    if !config.usage_accounting || object.contains_key("usage") {
        return false;
    }
    object.insert("usage".into(), json!({"include": true}));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::ProviderView;

    fn config(json: Value) -> OpenRouterConfig {
        OpenRouterConfig::from_view(ProviderView {
            id: "p",
            channel: super::super::ID,
            base_url: None,
            config: &json,
        })
        .unwrap()
    }

    #[test]
    fn the_model_override_beats_the_default_and_the_client_beats_both() {
        let preferences = config(json!({
            "provider": {"sort": "throughput"},
            "model_providers": {"anthropic/claude-sonnet-4": {"only": ["anthropic"]}}
        }));
        let mut default = json!({"model": "openai/gpt-5"});
        assert!(apply(&mut default, &preferences).unwrap());
        assert_eq!(default["provider"], json!({"sort": "throughput"}));

        let mut overridden = json!({"model": "anthropic/claude-sonnet-4"});
        assert!(apply(&mut overridden, &preferences).unwrap());
        assert_eq!(overridden["provider"], json!({"only": ["anthropic"]}));

        let mut client = json!({"model": "openai/gpt-5", "provider": {"order": ["azure"]}});
        assert!(
            apply(&mut client, &preferences).unwrap(),
            "usage was still added"
        );
        assert_eq!(
            client["provider"],
            json!({"order": ["azure"]}),
            "a routing choice the client made is never replaced"
        );
    }

    #[test]
    fn routing_preferences_that_are_not_an_object_are_a_configuration_error() {
        assert!(matches!(
            apply(
                &mut json!({"model": "m"}),
                &config(json!({"provider": "azure"}))
            ),
            Err(ChannelError::InvalidConfig(_))
        ));
    }

    #[test]
    fn usage_accounting_and_the_service_tier_are_opt_out() {
        let mut body = json!({"model": "m", "service_tier": "fast"});
        assert!(apply(&mut body, &config(json!({}))).unwrap());
        assert_eq!(body["service_tier"], "priority");
        assert_eq!(body["usage"], json!({"include": true}));

        let mut kept = json!({"service_tier": "fast", "usage": {"include": false}});
        assert!(
            !apply(
                &mut kept,
                &config(json!({"usage_accounting": false, "normalize_service_tier": false}))
            )
            .unwrap()
        );
        assert_eq!(
            kept,
            json!({"service_tier": "fast", "usage": {"include": false}})
        );
    }
}
