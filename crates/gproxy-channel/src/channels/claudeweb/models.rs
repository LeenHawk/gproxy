//! The model list claude.ai exposes to its front end, mined from the
//! organization's `claude_ai_bootstrap_models_config` at login
//! (v3 `claudeweb/models.rs`) and answered locally afterwards from the
//! credential metadata.

use std::collections::BTreeMap;

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Model {
    pub id: String,
    pub display_name: Option<String>,
}

/// Every `claude-*` id found anywhere in the config, sorted and deduplicated,
/// with the first display name seen next to it.
pub(super) fn from_config(config: &Value) -> Vec<Model> {
    let mut models = BTreeMap::<String, Option<String>>::new();
    collect(config, &mut models);
    models
        .into_iter()
        .map(|(id, display_name)| Model { id, display_name })
        .collect()
}

pub(super) fn to_metadata(models: &[Model]) -> Value {
    Value::Array(
        models
            .iter()
            .map(|model| {
                let mut entry = json!({"id": model.id});
                if let Some(name) = &model.display_name {
                    entry["display_name"] = Value::String(name.clone());
                }
                entry
            })
            .collect(),
    )
}

pub(super) fn from_metadata(metadata: &Value) -> Vec<Model> {
    metadata
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(Model {
                id: entry.get("id")?.as_str()?.to_owned(),
                display_name: entry
                    .get("display_name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// A Claude `GET /v1/models` reply. Only `id` and `display_name` are known;
/// capabilities and creation dates are not part of the bootstrap config.
pub(super) fn claude_list(models: &[Model]) -> Value {
    let data: Vec<Value> = models
        .iter()
        .map(|model| {
            json!({
                "type": "model",
                "id": model.id,
                "display_name": model.display_name.clone().unwrap_or_else(|| model.id.clone()),
            })
        })
        .collect();
    json!({
        "data": data,
        "has_more": false,
        "first_id": models.first().map(|m| m.id.as_str()),
        "last_id": models.last().map(|m| m.id.as_str()),
    })
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
