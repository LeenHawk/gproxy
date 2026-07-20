use serde_json::Value;

use super::canonicalize_claude_body;
use super::schema::{is_cacheable_block, is_cacheable_message_block};

#[derive(Clone, Copy)]
enum CacheScope<'a> {
    System,
    Message(Option<&'a str>),
}

pub(super) fn body(body: &mut Value) {
    canonicalize_claude_body(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };

    if let Some(Value::Array(blocks)) = root.get_mut("system") {
        let owned = std::mem::take(blocks);
        let cleaned = block_array(owned, &mut [], CacheScope::System);
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
                Some(Value::Array(blocks)) => block_array(
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

fn block_array(
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
                .map(|text| text.trim().to_string());
            if let Some(trimmed) = trimmed {
                if trimmed.is_empty() {
                    if let Some(control) = map.remove("cache_control")
                        && !attach_to_previous_in_scope(&mut out, &control, scope)
                    {
                        attach_to_previous_messages(prev_messages, &control);
                    }
                    continue;
                }
                map.insert("text".into(), Value::String(trimmed));
            }
        }
        out.push(Value::Object(map));
    }
    out
}

fn attach_to_previous_in_scope(out: &mut [Value], control: &Value, scope: CacheScope<'_>) -> bool {
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
            map.insert("cache_control".into(), control.clone());
        }
        return true;
    }
    false
}

fn attach_to_previous_messages(messages: &mut [Value], control: &Value) -> bool {
    for message in messages.iter_mut().rev() {
        let Some(map) = message.as_object_mut() else {
            continue;
        };
        let role = map.get("role").and_then(Value::as_str).map(str::to_owned);
        let Some(Value::Array(blocks)) = map.get_mut("content") else {
            continue;
        };
        if attach_to_previous_in_scope(
            blocks.as_mut_slice(),
            control,
            CacheScope::Message(role.as_deref()),
        ) {
            return true;
        }
    }
    false
}
