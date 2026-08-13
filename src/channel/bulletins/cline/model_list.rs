//! Normalize Cline's full model catalogue to the OpenAI model-list wire shape.

use bytes::Bytes;
use serde_json::Value;

pub(super) fn to_openai(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(root) = value.as_object_mut() else {
        return body;
    };
    if root.contains_key("error") {
        return body;
    }

    root.entry("object").or_insert_with(|| Value::from("list"));
    if let Some(models) = root.get_mut("data").and_then(Value::as_array_mut) {
        for model in models {
            let Some(model) = model.as_object_mut() else {
                continue;
            };
            model
                .entry("object")
                .or_insert_with(|| Value::from("model"));
            if !model.contains_key("owned_by") {
                let owner = model
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| id.split_once('/'))
                    .map_or("cline", |(owner, _)| owner);
                model.insert("owned_by".into(), Value::from(owner));
            }
        }
    }

    serde_json::to_vec(&value).map(Bytes::from).unwrap_or(body)
}
