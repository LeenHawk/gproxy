//! Always-on structural hygiene for Claude request bodies.
//!
//! Ported from v1 `utils/claude_cache_control.rs`: canonicalizes `system` /
//! `messages[].content` into block-array form, then drops whitespace-only text
//! blocks, empty content arrays, and empty messages — migrating any orphaned
//! `cache_control` marker onto a surviving cacheable block.

use serde_json::Value;

pub(super) fn canonicalize_claude_body(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };

    if let Some(system) = root.get_mut("system") {
        canonicalize_claude_system(system);
    }

    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            canonicalize_claude_message(message);
        }
    }
}

fn canonicalize_claude_system(system: &mut Value) {
    match system {
        Value::String(text) => {
            let text = std::mem::take(text);
            *system = Value::Array(vec![json_text_block(text.as_str())]);
        }
        Value::Array(blocks) => canonicalize_claude_blocks(blocks),
        _ => {}
    }
}

fn canonicalize_claude_message(message: &mut Value) {
    let Some(message_map) = message.as_object_mut() else {
        return;
    };
    let Some(content) = message_map.get_mut("content") else {
        return;
    };
    canonicalize_claude_content(content);
}

fn canonicalize_claude_content(content: &mut Value) {
    match content {
        Value::String(text) => {
            let text = std::mem::take(text);
            *content = Value::Array(vec![json_text_block(text.as_str())]);
        }
        Value::Object(_) => {
            let block = std::mem::take(content);
            *content = Value::Array(vec![block]);
        }
        Value::Array(blocks) => canonicalize_claude_blocks(blocks),
        _ => {}
    }
}

fn canonicalize_claude_blocks(blocks: &mut [Value]) {
    for block in blocks {
        if let Value::String(text) = block {
            let text = std::mem::take(text);
            *block = json_text_block(text.as_str());
        }
    }
}

fn json_text_block(text: &str) -> Value {
    serde_json::json!({
        "type": "text",
        "text": text,
    })
}

/// Check if a content block can have cache_control applied.
///
/// Blocks that CANNOT be cached:
/// - `thinking` blocks (must be cached indirectly via the assistant turn)
/// - Sub-content blocks like `citations` (cache the top-level document instead)
/// - Empty `text` blocks
fn is_cacheable_block(block: &serde_json::Map<String, Value>) -> bool {
    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
    match block_type {
        "thinking" => false,
        "citation" | "citations" | "char_location" | "page_location" | "content_block_location" => {
            false
        }
        "text" => {
            // Empty text blocks cannot be cached
            block
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|t| !t.is_empty())
        }
        _ => true,
    }
}

/// Remove whitespace-only text blocks, empty content arrays, and empty
/// messages. When a removed block carried `cache_control`, shift the marker
/// onto the most recent surviving cacheable block — first within the same
/// content/system array, then within previously kept messages. If no prior
/// cacheable block exists anywhere, the marker is dropped.
pub fn sanitize_claude_body(body: &mut Value) {
    canonicalize_claude_body(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };

    if let Some(Value::Array(blocks)) = root.get_mut("system") {
        let owned = std::mem::take(blocks);
        let cleaned = sanitize_block_array(owned, &mut []);
        if cleaned.is_empty() {
            root.remove("system");
        } else if let Some(Value::Array(target)) = root.get_mut("system") {
            *target = cleaned;
        }
    }

    if let Some(Value::Array(messages)) = root.get_mut("messages") {
        let owned = std::mem::take(messages);
        let mut kept: Vec<Value> = Vec::with_capacity(owned.len());
        for mut message in owned {
            let Some(message_map) = message.as_object_mut() else {
                kept.push(message);
                continue;
            };
            let cleaned_content = match message_map.remove("content") {
                Some(Value::Array(blocks)) => sanitize_block_array(blocks, kept.as_mut_slice()),
                Some(other) => {
                    message_map.insert("content".into(), other);
                    kept.push(Value::Object(message_map.clone()));
                    continue;
                }
                None => {
                    kept.push(Value::Object(message_map.clone()));
                    continue;
                }
            };
            if cleaned_content.is_empty() {
                continue;
            }
            message_map.insert("content".into(), Value::Array(cleaned_content));
            kept.push(Value::Object(message_map.clone()));
        }
        if let Some(Value::Array(target)) = root.get_mut("messages") {
            *target = kept;
        }
    }
}

fn sanitize_block_array(blocks: Vec<Value>, prev_messages: &mut [Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(blocks.len());
    for block in blocks {
        let Value::Object(mut map) = block else {
            out.push(block);
            continue;
        };
        let is_text = map.get("type").and_then(Value::as_str) == Some("text");
        if is_text {
            let trimmed = map
                .get("text")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string());
            if let Some(t) = trimmed {
                if t.is_empty() {
                    if let Some(cc) = map.remove("cache_control")
                        && !attach_cc_to_prev_in_scope(&mut out, &cc)
                    {
                        attach_cc_to_prev_messages(prev_messages, &cc);
                    }
                    continue;
                }
                map.insert("text".into(), Value::String(t));
            }
        }
        out.push(Value::Object(map));
    }
    out
}

fn attach_cc_to_prev_in_scope(out: &mut [Value], cc: &Value) -> bool {
    for block in out.iter_mut().rev() {
        let Some(map) = block.as_object_mut() else {
            continue;
        };
        if !is_cacheable_block(map) {
            continue;
        }
        if !map.contains_key("cache_control") {
            map.insert("cache_control".into(), cc.clone());
        }
        return true;
    }
    false
}

fn attach_cc_to_prev_messages(messages: &mut [Value], cc: &Value) -> bool {
    for message in messages.iter_mut().rev() {
        let Some(map) = message.as_object_mut() else {
            continue;
        };
        let Some(Value::Array(blocks)) = map.get_mut("content") else {
            continue;
        };
        if attach_cc_to_prev_in_scope(blocks.as_mut_slice(), cc) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod sanitize_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drops_empty_user_text_block_and_message() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": ""},
                {"role": "user", "content": "hi"}
            ]
        });
        sanitize_claude_body(&mut body);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["content"][0]["text"], "hi");
    }

    #[test]
    fn drops_whitespace_only_text_block() {
        let mut body = json!({
            "system": [
                {"type": "text", "text": "   \n"},
                {"type": "text", "text": "real"}
            ]
        });
        sanitize_claude_body(&mut body);
        let sys = body["system"].as_array().unwrap();
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0]["text"], "real");
    }

    #[test]
    fn shifts_cache_control_to_prev_block_in_same_array() {
        let mut body = json!({
            "system": [
                {"type": "text", "text": "anchor"},
                {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
            ]
        });
        sanitize_claude_body(&mut body);
        let sys = body["system"].as_array().unwrap();
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0]["text"], "anchor");
        assert_eq!(sys[0]["cache_control"]["ttl"], "5m");
    }

    #[test]
    fn shifts_cache_control_across_messages() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "first"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": " ", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });
        sanitize_claude_body(&mut body);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        let block = &messages[0]["content"][0];
        assert_eq!(block["text"], "first");
        assert_eq!(block["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn drops_cc_when_no_prior_cacheable_block_exists() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });
        sanitize_claude_body(&mut body);
        assert!(body["messages"].as_array().unwrap().is_empty());
    }

    #[test]
    fn removes_system_field_when_all_blocks_drop() {
        let mut body = json!({
            "system": [{"type": "text", "text": "  "}],
            "messages": [{"role": "user", "content": "hi"}]
        });
        sanitize_claude_body(&mut body);
        assert!(body.get("system").is_none());
    }

    #[test]
    fn preserves_non_text_blocks() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "image", "source": {"type": "base64", "data": "x"}},
                    {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });
        sanitize_claude_body(&mut body);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn trims_text_when_kept() {
        let mut body = json!({
            "messages": [{"role": "user", "content": "  hi  "}]
        });
        sanitize_claude_body(&mut body);
        assert_eq!(body["messages"][0]["content"][0]["text"], "hi");
    }
}
