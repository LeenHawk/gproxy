//! Claude Web model catalog extracted from bootstrap metadata.

use std::collections::BTreeMap;

use bytes::Bytes;
use serde_json::{Map, Value, json};

pub(super) fn credential_models(secret: &Value) -> Option<Bytes> {
    let catalog = secret.get("model_catalog")?;
    serde_json::to_vec(catalog).ok().map(Bytes::from)
}

pub(super) fn catalog(config: &Value) -> Option<Value> {
    let mut models = BTreeMap::<String, Option<String>>::new();
    collect(config, &mut models);
    if models.is_empty() {
        return None;
    }
    let data = models
        .into_iter()
        .map(|(id, display_name)| {
            let mut model = Map::from_iter([("id".into(), Value::String(id))]);
            if let Some(display_name) = display_name {
                model.insert("display_name".into(), Value::String(display_name));
            }
            Value::Object(model)
        })
        .collect::<Vec<_>>();
    let first_id = data.first().and_then(|model| model.get("id")).cloned();
    let last_id = data.last().and_then(|model| model.get("id")).cloned();
    Some(json!({
        "data": data,
        "first_id": first_id,
        "last_id": last_id,
        "has_more": false,
    }))
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
                .filter(|id| is_claude_model_id(id));
            let display_name = ["display_name", "displayName", "label", "name"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(Value::as_str))
                .filter(|name| !is_claude_model_id(name))
                .map(str::to_owned);
            if let Some(id) = id {
                models
                    .entry(id.to_owned())
                    .and_modify(|current| {
                        if current.is_none() {
                            *current = display_name.clone();
                        }
                    })
                    .or_insert(display_name);
            }
            for (key, child) in object {
                if is_claude_model_id(key) {
                    let display = child
                        .as_object()
                        .and_then(|object| {
                            ["display_name", "displayName", "label", "name"]
                                .into_iter()
                                .find_map(|field| object.get(field).and_then(Value::as_str))
                        })
                        .filter(|name| !is_claude_model_id(name))
                        .map(str::to_owned);
                    models.entry(key.clone()).or_insert(display);
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

fn is_claude_model_id(value: &str) -> bool {
    value.starts_with("claude-") && value.len() > "claude-".len()
}
