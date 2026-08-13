//! Merge Cline's curated model groups into the OpenAI model-list wire shape.

use std::collections::HashSet;

use bytes::Bytes;
use serde_json::{Map, Value};

const GROUPS: [&str; 3] = ["recommended", "free", "clinePass"];

pub(super) fn to_openai(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(root) = value.as_object() else {
        return body;
    };
    if root.contains_key("error") {
        return body;
    }

    let mut seen = HashSet::new();
    let mut data = Vec::new();
    for group in GROUPS {
        let Some(models) = root.get(group).and_then(Value::as_array) else {
            continue;
        };
        for model in models {
            let Some(mut model) = model.as_object().cloned() else {
                continue;
            };
            let Some(id) = model.get("id").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            model.insert("object".into(), Value::from("model"));
            model.insert("owned_by".into(), Value::from(owner(&id)));
            model.insert("cline_group".into(), Value::from(group));
            data.push(Value::Object(model));
        }
    }

    let mut output = Map::new();
    output.insert("object".into(), Value::from("list"));
    output.insert("data".into(), Value::Array(data));
    serde_json::to_vec(&Value::Object(output))
        .map(Bytes::from)
        .unwrap_or(body)
}

fn owner(id: &str) -> &str {
    id.split_once('/').map_or("cline", |(owner, _)| owner)
}
