use bytes::Bytes;
use serde_json::{Value, json};

use crate::TransformError;

pub(crate) fn request(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let source: Value = serde_json::from_slice(&body)?;
    let instructions = source.get("instructions").cloned();
    let converted = crate::content::responses_to_claude(body, model, false)?;
    let mut value: Value = serde_json::from_slice(&converted)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| TransformError::shape("Claude compact request", "root must be an object"))?;
    object.remove("stream");
    object.insert("max_tokens".into(), Value::from(4096));
    object.insert(
        "context_management".into(),
        json!({
            "edits":[{
                "type":"compact_20260112",
                "instructions":instructions,
                "pause_after_compaction":true
            }]
        }),
    );
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}

pub(crate) fn response(body: Bytes) -> Result<Bytes, TransformError> {
    let converted = crate::content::claude_to_responses_response(body)?;
    let mut value: Value = serde_json::from_slice(&converted)?;
    let object = value.as_object_mut().ok_or_else(|| {
        TransformError::shape("OpenAI compact response", "root must be an object")
    })?;
    object.insert("object".into(), Value::String("response.compaction".into()));
    object.remove("completed_at");
    object.remove("incomplete_details");
    object.remove("model");
    object.remove("output_text");
    object.remove("status");
    if let Some(output) = object.get_mut("output").and_then(Value::as_array_mut) {
        for item in output {
            if item.get("type").and_then(Value::as_str) == Some("message")
                && let Some(parts) = item.get_mut("content").and_then(Value::as_array_mut)
            {
                for part in parts {
                    if part.get("type").and_then(Value::as_str) == Some("output_text") {
                        part["type"] = Value::String("text".into());
                        part.as_object_mut().map(|part| part.remove("annotations"));
                    }
                }
            }
        }
    }
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}
