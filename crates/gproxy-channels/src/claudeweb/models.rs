use std::collections::BTreeMap;

use gproxy_channel_api::ModelInfo;
use serde_json::Value;

pub(super) fn from_secret(secret: &Value) -> Vec<ModelInfo> {
    let mut models = BTreeMap::<String, Option<String>>::new();
    if let Some(config) = secret.get("claude_ai_bootstrap_models_config") {
        collect(config, &mut models);
    }
    models
        .into_iter()
        .map(|(id, display_name)| ModelInfo {
            id,
            display_name,
            context_window: None,
            max_output_tokens: None,
            thinking_supported: None,
            thinking_adaptive_supported: None,
            thinking_enabled_supported: None,
            metadata: Default::default(),
        })
        .collect()
}

fn collect(value: &Value, models: &mut BTreeMap<String, Option<String>>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect(item, models);
            }
        }
        Value::Object(object) => {
            let id = ["id", "model", "model_id", "value"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(Value::as_str))
                .filter(|id| is_model_id(id));
            let display_name = ["display_name", "displayName", "label", "name"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(Value::as_str))
                .filter(|name| !is_model_id(name))
                .map(str::to_owned);
            if let Some(id) = id {
                merge(models, id, display_name);
            }
            for (key, child) in object {
                if is_model_id(key) {
                    let display_name = child
                        .as_object()
                        .and_then(|value| {
                            ["display_name", "displayName", "label", "name"]
                                .into_iter()
                                .find_map(|field| value.get(field).and_then(Value::as_str))
                        })
                        .filter(|name| !is_model_id(name))
                        .map(str::to_owned);
                    merge(models, key, display_name);
                }
                collect(child, models);
            }
        }
        Value::String(text) => {
            if let Ok(nested) = serde_json::from_str::<Value>(text) {
                collect(&nested, models);
            }
        }
        _ => {}
    }
}

fn merge(models: &mut BTreeMap<String, Option<String>>, id: &str, display_name: Option<String>) {
    models
        .entry(id.into())
        .and_modify(|current| {
            if current.is_none() {
                *current = display_name.clone();
            }
        })
        .or_insert(display_name);
}

fn is_model_id(value: &str) -> bool {
    value.starts_with("claude-") && value.len() > "claude-".len()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn extracts_sorted_deduplicated_models_from_bootstrap_shapes() {
        let secret = json!({
            "claude_ai_bootstrap_models_config": {
                "claude-z": { "displayName": "Claude Z" },
                "nested": [
                    { "model_id": "claude-a", "label": "Claude A" },
                    "{\"model\":\"claude-z\"}",
                    { "model": "not-claude" }
                ]
            }
        });
        let models = super::from_secret(&secret);
        assert_eq!(
            models
                .iter()
                .map(|model| (model.id.as_str(), model.display_name.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("claude-a", Some("Claude A")),
                ("claude-z", Some("Claude Z"))
            ]
        );
    }
}
