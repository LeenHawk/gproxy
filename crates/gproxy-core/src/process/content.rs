use gproxy_protocol::ContentGenerationKind;
use serde_json::{Value, json};

use super::{CacheBreakpointConfig, TextPosition};

pub(super) fn system_text(
    body: &mut Value,
    kind: Option<ContentGenerationKind>,
    text: &str,
    position: TextPosition,
) -> bool {
    let Some(object) = body.as_object_mut() else {
        return false;
    };
    match kind {
        Some(ContentGenerationKind::ClaudeMessages) => inject_text(
            object.entry("system").or_insert(Value::Null),
            text,
            position,
        ),
        Some(ContentGenerationKind::OpenAiChat) => {
            let Some(Value::Array(messages)) = object.get_mut("messages") else {
                return false;
            };
            let message = json!({"role": "system", "content": text});
            match position {
                TextPosition::Prepend => messages.insert(0, message),
                TextPosition::Append => {
                    let index = messages
                        .iter()
                        .take_while(|message| {
                            message.get("role").and_then(Value::as_str) == Some("system")
                        })
                        .count();
                    messages.insert(index, message);
                }
            }
            true
        }
        Some(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => inject_text(
            object.entry("instructions").or_insert(Value::Null),
            text,
            position,
        ),
        Some(ContentGenerationKind::GeminiGenerateContent) => {
            let system = object
                .entry("systemInstruction")
                .or_insert_with(|| json!({"parts": []}));
            let Some(system) = system.as_object_mut() else {
                return false;
            };
            let parts = system.entry("parts").or_insert_with(|| json!([]));
            let Some(parts) = parts.as_array_mut() else {
                return false;
            };
            let part = json!({"text": text});
            match position {
                TextPosition::Prepend => parts.insert(0, part),
                TextPosition::Append => parts.push(part),
            }
            true
        }
        None => false,
    }
}

fn inject_text(value: &mut Value, text: &str, position: TextPosition) -> bool {
    match value {
        Value::Null => {
            *value = Value::String(text.into());
            true
        }
        Value::String(current) => {
            *current = match position {
                TextPosition::Prepend => format!("{text} {current}"),
                TextPosition::Append => format!("{current}\n\n{text}"),
            };
            true
        }
        Value::Array(parts) => {
            let part = json!({"type": "text", "text": text});
            match position {
                TextPosition::Prepend => parts.insert(0, part),
                TextPosition::Append => parts.push(part),
            }
            true
        }
        _ => false,
    }
}

pub(super) fn cache_breakpoint(
    body: &mut Value,
    kind: Option<ContentGenerationKind>,
    config: &CacheBreakpointConfig,
) -> bool {
    match kind {
        Some(ContentGenerationKind::ClaudeMessages) => claude_breakpoint(body, config),
        Some(ContentGenerationKind::OpenAiChat) => openai_chat_breakpoint(body, config),
        Some(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => openai_responses_breakpoint(body, config),
        Some(ContentGenerationKind::GeminiGenerateContent) | None => false,
    }
}

fn marker(ttl: Option<&str>) -> Value {
    let mut value = json!({"type": "ephemeral"});
    if let Some(ttl) = ttl {
        value["ttl"] = Value::String(ttl.into());
    }
    value
}

fn claude_breakpoint(body: &mut Value, config: &CacheBreakpointConfig) -> bool {
    if config.target == "top_level" {
        let Some(object) = body.as_object_mut() else {
            return false;
        };
        object.insert("cache_control".into(), marker(config.ttl.as_deref()));
        return true;
    }
    let target = match config.target.as_str() {
        "system" => body.get_mut("system"),
        "tools" => body.get_mut("tools"),
        "message" => body
            .get_mut("messages")
            .and_then(Value::as_array_mut)
            .and_then(|messages| messages.last_mut())
            .and_then(|message| message.get_mut("content")),
        _ => None,
    };
    mark_block(
        target,
        config.index,
        "cache_control",
        marker(config.ttl.as_deref()),
    )
}

fn openai_chat_breakpoint(body: &mut Value, config: &CacheBreakpointConfig) -> bool {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };
    let candidates = messages
        .iter_mut()
        .filter(|message| match config.target.as_str() {
            "system" => message.get("role").and_then(Value::as_str) == Some("system"),
            "message" => true,
            _ => false,
        })
        .collect::<Vec<_>>();
    let Some(message) = select_mut(candidates, config.index) else {
        return false;
    };
    let Some(content) = message.get_mut("content") else {
        return false;
    };
    let changed = mark_openai_content(content);
    if changed && let Some(ttl) = &config.ttl {
        body["prompt_cache_options"]["ttl"] = Value::String(ttl.clone());
    }
    changed
}

fn openai_responses_breakpoint(body: &mut Value, config: &CacheBreakpointConfig) -> bool {
    let target = match config.target.as_str() {
        "system" => body.get_mut("instructions"),
        "message" => body.get_mut("input"),
        _ => None,
    };
    let changed = mark_openai_content(target.unwrap_or(&mut Value::Null));
    if changed && let Some(ttl) = &config.ttl {
        body["prompt_cache_options"]["ttl"] = Value::String(ttl.clone());
    }
    changed
}

fn mark_openai_content(content: &mut Value) -> bool {
    if let Value::String(text) = content {
        *content = json!([{"type": "text", "text": std::mem::take(text), "prompt_cache_breakpoint": {"mode": "explicit"}}]);
        return true;
    }
    mark_block(
        Some(content),
        None,
        "prompt_cache_breakpoint",
        json!({"mode": "explicit"}),
    )
}

fn mark_block(target: Option<&mut Value>, index: Option<i64>, field: &str, marker: Value) -> bool {
    let Some(target) = target else { return false };
    if let Value::String(text) = target {
        *target = json!([{"type": "text", "text": std::mem::take(text)}]);
    }
    let Some(array) = target.as_array_mut() else {
        return false;
    };
    let Some(block) = index_value(array.len(), index)
        .and_then(|index| array.get_mut(index))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    block.insert(field.into(), marker);
    true
}

fn index_value(length: usize, index: Option<i64>) -> Option<usize> {
    let index = index.unwrap_or(-1);
    if index < 0 {
        length.checked_sub(index.unsigned_abs() as usize)
    } else {
        usize::try_from(index).ok().filter(|index| *index < length)
    }
}

fn select_mut<T>(mut values: Vec<&mut T>, index: Option<i64>) -> Option<&mut T> {
    let index = index_value(values.len(), index)?;
    Some(values.swap_remove(index))
}
