use serde_json::{Map, Value, json};

pub(super) fn json_text_block(text: &str) -> Value {
    json!({
        "type": "text",
        "text": text,
    })
}

/// Check if a content block can have cache_control applied.
pub(super) fn is_cacheable_block(block: &Map<String, Value>) -> bool {
    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
    match block_type {
        "thinking" | "redacted_thinking" => false,
        "citation" | "citations" | "char_location" | "page_location" | "content_block_location" => {
            false
        }
        "text" => block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty()),
        _ => true,
    }
}

pub(super) fn is_cacheable_message_block(role: Option<&str>, block: &Map<String, Value>) -> bool {
    if !is_cacheable_block(block) {
        return false;
    }
    match block.get("type").and_then(Value::as_str) {
        Some("image" | "document") => role == Some("user"),
        _ => true,
    }
}
