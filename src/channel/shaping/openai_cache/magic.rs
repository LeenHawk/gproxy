use serde_json::{Map, Value};

use super::super::claude_magic_cache;
use super::mutation::prepend_instruction_anchor;
use super::schema::{
    PartFamily, chat_text_part, explicit_breakpoint, response_input_text_part, response_message,
};

pub(super) fn apply_chat(root: &mut Map<String, Value>, remaining: &mut usize) {
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        let Some(message) = message.as_object_mut() else {
            continue;
        };
        let supported_string = !matches!(
            message.get("role").and_then(Value::as_str),
            Some("function") | None
        );
        let Some(content) = message.get_mut("content") else {
            continue;
        };
        if supported_string && let Some(text) = take_magic_string(content, remaining) {
            *content = Value::Array(vec![chat_text_part(text, true)]);
            continue;
        }
        if let Value::Array(parts) = content {
            apply_to_parts(parts, PartFamily::Chat, remaining);
        }
    }
}

pub(super) fn apply_responses(root: &mut Map<String, Value>, remaining: &mut usize) {
    let instruction_triggered = root
        .get_mut("instructions")
        .and_then(|value| match value {
            Value::String(text) => Some(text),
            _ => None,
        })
        .is_some_and(claude_magic_cache::strip_magic_tokens);
    if instruction_triggered && *remaining > 0 && prepend_instruction_anchor(root) {
        *remaining -= 1;
    }

    if let Some(variables) = root
        .get_mut("prompt")
        .and_then(Value::as_object_mut)
        .and_then(|prompt| prompt.get_mut("variables"))
        .and_then(Value::as_object_mut)
    {
        for value in variables.values_mut() {
            if let Some(text) = take_magic_string(value, remaining) {
                *value = response_input_text_part(text, true);
                continue;
            }
            if let Value::Object(part) = value {
                apply_to_part(part, PartFamily::Responses, remaining);
            }
        }
    }

    let Some(input) = root.get_mut("input") else {
        return;
    };
    if let Some(text) = take_magic_string(input, remaining) {
        *input = Value::Array(vec![response_message(text, true)]);
        return;
    }
    if let Value::Array(items) = input {
        for item in items {
            let Some(item) = item.as_object_mut() else {
                continue;
            };
            for field in ["content", "output"] {
                let Some(content) = item.get_mut(field) else {
                    continue;
                };
                if let Some(text) = take_magic_string(content, remaining) {
                    *content = Value::Array(vec![response_input_text_part(text, true)]);
                    continue;
                }
                if let Value::Array(parts) = content {
                    apply_to_parts(parts, PartFamily::Responses, remaining);
                }
            }
        }
    }
}

fn take_magic_string(value: &mut Value, remaining: &mut usize) -> Option<String> {
    let Value::String(text) = value else {
        return None;
    };
    if !claude_magic_cache::strip_magic_tokens(text) || text.trim().is_empty() || *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    Some(std::mem::take(text))
}

fn apply_to_parts(parts: &mut [Value], family: PartFamily, remaining: &mut usize) {
    for part in parts {
        if let Some(part) = part.as_object_mut() {
            apply_to_part(part, family, remaining);
        }
    }
}

fn apply_to_part(part: &mut Map<String, Value>, family: PartFamily, remaining: &mut usize) {
    let text_key = match (family, part.get("type").and_then(Value::as_str)) {
        (PartFamily::Chat, Some("text")) => "text",
        (PartFamily::Chat, Some("refusal")) => "refusal",
        (PartFamily::Responses, Some("input_text")) => "text",
        _ => return,
    };
    let has_breakpoint = part.contains_key("prompt_cache_breakpoint");
    let Some(text) = part.get_mut(text_key).and_then(|value| match value {
        Value::String(text) => Some(text),
        _ => None,
    }) else {
        return;
    };
    if !claude_magic_cache::strip_magic_tokens(text)
        || text.trim().is_empty()
        || *remaining == 0
        || has_breakpoint
    {
        return;
    }
    part.insert("prompt_cache_breakpoint".into(), explicit_breakpoint());
    *remaining -= 1;
}
