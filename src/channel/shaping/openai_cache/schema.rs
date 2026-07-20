use serde_json::{Map, Value, json};

#[derive(Clone, Copy)]
pub(super) enum PartFamily {
    Chat,
    Responses,
}

pub(super) fn is_supported_chat_part(part: &Map<String, Value>) -> bool {
    matches!(
        part.get("type").and_then(Value::as_str),
        Some("text" | "image_url" | "input_audio" | "file" | "refusal")
    )
}

pub(super) fn is_supported_response_part(part: &Map<String, Value>) -> bool {
    matches!(
        part.get("type").and_then(Value::as_str),
        Some("input_text" | "input_image" | "input_file")
    )
}

pub(super) fn is_cacheable_chat_part(part: &Map<String, Value>) -> bool {
    if !is_supported_chat_part(part) {
        return false;
    }
    match part.get("type").and_then(Value::as_str) {
        Some("text") => part
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty()),
        Some("refusal") => part
            .get("refusal")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty()),
        _ => true,
    }
}

pub(super) fn is_cacheable_response_part(part: &Map<String, Value>) -> bool {
    if !is_supported_response_part(part) {
        return false;
    }
    match part.get("type").and_then(Value::as_str) {
        Some("input_text") => part
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty()),
        _ => true,
    }
}

pub(super) fn explicit_breakpoint() -> Value {
    json!({"mode": "explicit"})
}

pub(super) fn chat_text_part(text: String, breakpoint: bool) -> Value {
    let mut part = json!({"type": "text", "text": text});
    if breakpoint {
        part["prompt_cache_breakpoint"] = explicit_breakpoint();
    }
    part
}

pub(super) fn response_input_text_part(text: String, breakpoint: bool) -> Value {
    let mut part = json!({"type": "input_text", "text": text});
    if breakpoint {
        part["prompt_cache_breakpoint"] = explicit_breakpoint();
    }
    part
}

pub(super) fn response_message(text: String, breakpoint: bool) -> Value {
    response_message_with_role("user", text, breakpoint)
}

pub(super) fn response_message_with_role(role: &str, text: String, breakpoint: bool) -> Value {
    json!({
        "type": "message",
        "role": role,
        "content": [response_input_text_part(text, breakpoint)]
    })
}
