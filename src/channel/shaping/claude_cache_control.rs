//! Always-on structural hygiene for Claude request bodies.
//!
//! Ported from v1 `utils/claude_cache_control.rs`: canonicalizes `system` /
//! `messages[].content` into block-array form, then drops whitespace-only text
//! blocks, empty content arrays, and empty messages — migrating any orphaned
//! `cache_control` marker onto a surviving cacheable block.

use serde_json::{Value, json};

const MAX_BREAKPOINTS: usize = 4;

#[derive(Clone, Copy)]
enum CacheLocation {
    Tool(usize),
    System(usize),
    Message {
        message_index: usize,
        block_index: usize,
    },
}

#[derive(Clone, Copy)]
enum CacheScope<'a> {
    System,
    Message(Option<&'a str>),
}

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
        "thinking" | "redacted_thinking" => false,
        "citation" | "citations" | "char_location" | "page_location" | "content_block_location" => {
            false
        }
        "text" => {
            // Empty text blocks cannot be cached
            block
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|t| !t.trim().is_empty())
        }
        _ => true,
    }
}

fn is_cacheable_message_block(role: Option<&str>, block: &serde_json::Map<String, Value>) -> bool {
    if !is_cacheable_block(block) {
        return false;
    }
    match block.get("type").and_then(Value::as_str) {
        Some("image" | "document") => role == Some("user"),
        _ => true,
    }
}

/// Apply a provider rule to a Claude body after normalizing string content to
/// block arrays. Message indexes address one flat sequence of cacheable blocks
/// across all messages, in prompt order.
pub fn apply_manual_cache_breakpoint(
    body: &mut Value,
    target: &str,
    index: Option<i64>,
    ttl: Option<&str>,
) -> Result<(), &'static str> {
    canonicalize_claude_body(body);
    let root = body.as_object_mut().ok_or("body not an object")?;
    let control = match ttl {
        Some(ttl) => json!({"type": "ephemeral", "ttl": ttl}),
        None => json!({"type": "ephemeral"}),
    };

    if matches!(target, "top_level" | "global") {
        if root.contains_key("cache_control") {
            return Ok(());
        }
        if existing_cache_breakpoint_count(root) >= MAX_BREAKPOINTS {
            return Err("Claude cache breakpoint limit reached");
        }
        root.insert("cache_control".into(), control);
        return Ok(());
    }

    let locations = match target {
        "tools" => root
            .get("tools")
            .and_then(Value::as_array)
            .map(|tools| {
                tools
                    .iter()
                    .enumerate()
                    .filter(|(_, tool)| tool.is_object())
                    .map(|(index, _)| CacheLocation::Tool(index))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        "system" => root
            .get("system")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, block)| block.as_object().is_some_and(is_cacheable_block))
                    .map(|(index, _)| CacheLocation::System(index))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        "message" => message_cache_locations(root),
        _ => return Err("unsupported cache breakpoint target"),
    };
    let location = resolve_location(&locations, index)?;
    let existing_count = existing_cache_breakpoint_count(root);
    let block = match location {
        CacheLocation::Tool(index) => root
            .get_mut("tools")
            .and_then(Value::as_array_mut)
            .and_then(|tools| tools.get_mut(index)),
        CacheLocation::System(index) => root
            .get_mut("system")
            .and_then(Value::as_array_mut)
            .and_then(|blocks| blocks.get_mut(index)),
        CacheLocation::Message {
            message_index,
            block_index,
        } => root
            .get_mut("messages")
            .and_then(Value::as_array_mut)
            .and_then(|messages| messages.get_mut(message_index))
            .and_then(|message| message.get_mut("content"))
            .and_then(Value::as_array_mut)
            .and_then(|blocks| blocks.get_mut(block_index)),
    }
    .and_then(Value::as_object_mut)
    .ok_or("target cache block not found")?;

    if block.contains_key("cache_control") {
        return Ok(());
    }
    if existing_count >= MAX_BREAKPOINTS {
        return Err("Claude cache breakpoint limit reached");
    }
    block.insert("cache_control".into(), control);
    Ok(())
}

fn message_cache_locations(root: &serde_json::Map<String, Value>) -> Vec<CacheLocation> {
    let mut locations = Vec::new();
    let Some(messages) = root.get("messages").and_then(Value::as_array) else {
        return locations;
    };
    for (message_index, message) in messages.iter().enumerate() {
        let role = message.get("role").and_then(Value::as_str);
        let Some(blocks) = message.get("content").and_then(Value::as_array) else {
            continue;
        };
        for (block_index, block) in blocks.iter().enumerate() {
            if block
                .as_object()
                .is_some_and(|block| is_cacheable_message_block(role, block))
            {
                locations.push(CacheLocation::Message {
                    message_index,
                    block_index,
                });
            }
        }
    }
    locations
}

fn resolve_location<T: Copy>(locations: &[T], index: Option<i64>) -> Result<T, &'static str> {
    let resolved = resolve_block_index(locations.len(), index)
        .ok_or("index out of range or no cacheable block")?;
    Ok(locations[resolved])
}

fn resolve_block_index(len: usize, index: Option<i64>) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match index {
        None => Some(len - 1),
        Some(0) => None,
        Some(index) if index > 0 => {
            let nth = usize::try_from(index).ok()?;
            (nth <= len).then(|| nth - 1)
        }
        Some(index) => {
            let from_end = usize::try_from(index.unsigned_abs()).ok()?;
            (from_end <= len).then(|| len - from_end)
        }
    }
}

pub(super) fn existing_cache_breakpoint_count(root: &serde_json::Map<String, Value>) -> usize {
    let mut count = usize::from(root.contains_key("cache_control"));
    for field in ["tools", "system"] {
        if let Some(blocks) = root.get(field).and_then(Value::as_array) {
            count += blocks
                .iter()
                .filter(|block| block.get("cache_control").is_some())
                .count();
        }
    }
    if let Some(messages) = root.get("messages").and_then(Value::as_array) {
        for message in messages {
            if let Some(blocks) = message.get("content").and_then(Value::as_array) {
                count += blocks
                    .iter()
                    .filter(|block| block.get("cache_control").is_some())
                    .count();
            }
        }
    }
    count
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
        let cleaned = sanitize_block_array(owned, &mut [], CacheScope::System);
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
            let role = message_map
                .get("role")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let cleaned_content = match message_map.remove("content") {
                Some(Value::Array(blocks)) => sanitize_block_array(
                    blocks,
                    kept.as_mut_slice(),
                    CacheScope::Message(role.as_deref()),
                ),
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

fn sanitize_block_array(
    blocks: Vec<Value>,
    prev_messages: &mut [Value],
    scope: CacheScope<'_>,
) -> Vec<Value> {
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
                        && !attach_cc_to_prev_in_scope(&mut out, &cc, scope)
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

fn attach_cc_to_prev_in_scope(out: &mut [Value], cc: &Value, scope: CacheScope<'_>) -> bool {
    for block in out.iter_mut().rev() {
        let Some(map) = block.as_object_mut() else {
            continue;
        };
        let cacheable = match scope {
            CacheScope::System => is_cacheable_block(map),
            CacheScope::Message(role) => is_cacheable_message_block(role, map),
        };
        if !cacheable {
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
        let role = map.get("role").and_then(Value::as_str).map(str::to_owned);
        let Some(Value::Array(blocks)) = map.get_mut("content") else {
            continue;
        };
        if attach_cc_to_prev_in_scope(
            blocks.as_mut_slice(),
            cc,
            CacheScope::Message(role.as_deref()),
        ) {
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
    fn manual_message_normalizes_and_indexes_flat_cacheable_blocks() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": "first"},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "secret", "signature": "sig"},
                    {"type": "text", "text": "second"}
                ]},
                {"role": "user", "content": [{"type": "text", "text": "   "}]}
            ]
        });

        apply_manual_cache_breakpoint(&mut body, "message", Some(-1), Some("5m")).unwrap();

        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert!(
            body["messages"][1]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert_eq!(
            body["messages"][1]["content"][1]["cache_control"]["ttl"],
            "5m"
        );
    }

    #[test]
    fn manual_cache_breakpoint_preserves_existing_and_enforces_four_slots() {
        let mut body = json!({
            "cache_control": {"type": "ephemeral"},
            "system": [{"type": "text", "text": "sys", "cache_control": {"type": "ephemeral"}}],
            "tools": [{"name": "a", "cache_control": {"type": "ephemeral"}}],
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "kept", "cache_control": {"type": "ephemeral", "ttl": "1h"}},
                {"type": "text", "text": "new"}
            ]}]
        });

        apply_manual_cache_breakpoint(&mut body, "message", Some(1), Some("5m")).unwrap();
        assert_eq!(
            body["messages"][0]["content"][0]["cache_control"]["ttl"],
            "1h"
        );
        assert_eq!(
            apply_manual_cache_breakpoint(&mut body, "message", Some(2), None),
            Err("Claude cache breakpoint limit reached")
        );
        assert!(
            body["messages"][0]["content"][1]
                .get("cache_control")
                .is_none()
        );
    }

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
    fn does_not_shift_cache_control_to_assistant_image() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "anchor"}]},
                {"role": "assistant", "content": [
                    {"type": "image", "source": {"type": "base64", "data": "x"}},
                    {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });

        sanitize_claude_body(&mut body);

        assert!(
            body["messages"][1]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert_eq!(
            body["messages"][0]["content"][0]["cache_control"]["type"],
            "ephemeral"
        );
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
