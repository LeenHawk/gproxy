use bytes::Bytes;
use serde_json::{Value, json};

const COMPACTION_BETA: &str = "compact-2026-01-12";

pub(super) fn is_request(body: &Bytes) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.pointer("/context_management/edits").cloned())
        .and_then(|value| value.as_array().cloned())
        .is_some_and(|edits| {
            edits
                .iter()
                .any(|edit| edit.get("type").and_then(Value::as_str) == Some("compact_20260112"))
        })
}

pub(super) fn request(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(root) = value.as_object_mut() else {
        return body;
    };
    root.remove("model");
    root.remove("stream");
    root.insert(
        "anthropic_version".into(),
        Value::String("bedrock-2023-05-31".into()),
    );
    let beta = root.entry("anthropic_beta").or_insert_with(|| json!([]));
    if let Some(values) = beta.as_array_mut()
        && !values
            .iter()
            .any(|value| value.as_str() == Some(COMPACTION_BETA))
    {
        values.push(Value::String(COMPACTION_BETA.into()));
    }
    Bytes::from(value.to_string())
}
