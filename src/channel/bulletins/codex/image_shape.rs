//! Normalize OpenAI Images requests to the narrower Codex subscription schema.

use bytes::Bytes;
use serde_json::{Map, Value, json};

const CREATE_KEYS: &[&str] = &["prompt", "background", "model", "n", "quality", "size"];
const EDIT_KEYS: &[&str] = &["prompt", "background", "model", "n", "quality", "size"];

pub(super) fn create(body: Bytes) -> Bytes {
    filter_object(body, CREATE_KEYS, false)
}

pub(super) fn edit(body: Bytes) -> Bytes {
    filter_object(body, EDIT_KEYS, true)
}

fn filter_object(body: Bytes, allowed: &[&str], images: bool) -> Bytes {
    let Ok(Value::Object(mut input)) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let mut output = Map::new();
    for key in allowed {
        if let Some(value) = input.remove(*key) {
            output.insert((*key).to_owned(), value);
        }
    }
    if images {
        let value = input.remove("images").or_else(|| input.remove("image"));
        if let Some(value) = value {
            output.insert("images".to_owned(), normalize_images(value));
        }
    }
    serde_json::to_vec(&Value::Object(output))
        .map(Bytes::from)
        .unwrap_or(body)
}

fn normalize_images(value: Value) -> Value {
    let values = match value {
        Value::Array(values) => values,
        value => vec![value],
    };
    Value::Array(
        values
            .into_iter()
            .map(|value| match value {
                Value::String(image_url) => json!({ "image_url": image_url }),
                Value::Object(object) => Value::Object(object),
                other => other,
            })
            .collect(),
    )
}
