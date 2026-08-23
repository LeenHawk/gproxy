use bytes::Bytes;
use serde_json::Value;

use crate::TransformError;

pub(crate) fn openai_to_claude(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let converted = crate::content::responses_to_claude(body, model, false)?;
    let mut value: Value = serde_json::from_slice(&converted)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| TransformError::shape("Claude count_tokens", "root must be an object"))?;
    object.remove("max_tokens");
    object.remove("stream");
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}

pub(crate) fn claude_to_openai(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let converted = crate::content::claude_to_responses(body, model, false)?;
    let mut value: Value = serde_json::from_slice(&converted)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| TransformError::shape("OpenAI input_tokens", "root must be an object"))?;
    for field in ["max_output_tokens", "stream", "temperature", "top_p"] {
        object.remove(field);
    }
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}

pub(crate) fn claude_to_openai_response(body: Bytes) -> Result<Bytes, TransformError> {
    let value: Value = serde_json::from_slice(&body)?;
    let input_tokens = value
        .get("input_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| TransformError::shape("Claude count_tokens", "input_tokens is missing"))?;
    Ok(Bytes::from(serde_json::to_vec(&serde_json::json!({
        "object": "response.input_tokens",
        "input_tokens": input_tokens.min(u64::from(u32::MAX)),
    }))?))
}

pub(crate) fn openai_to_claude_response(body: Bytes) -> Result<Bytes, TransformError> {
    let value: Value = serde_json::from_slice(&body)?;
    let input_tokens = value
        .get("input_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| TransformError::shape("OpenAI input_tokens", "input_tokens is missing"))?;
    Ok(Bytes::from(serde_json::to_vec(
        &serde_json::json!({"input_tokens": input_tokens}),
    )?))
}
