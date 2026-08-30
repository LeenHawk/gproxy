use gproxy_protocol::ContentGenerationKind;
use serde_json::{Map, Value, json};

pub(crate) fn apply(body: &mut Value, kind: ContentGenerationKind) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let mut remaining = 4;
    match kind {
        ContentGenerationKind::OpenAiChat => chat(root, &mut remaining),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => responses(root, &mut remaining),
        ContentGenerationKind::ClaudeMessages | ContentGenerationKind::GeminiGenerateContent => {}
    }
}

fn chat(root: &mut Map<String, Value>, remaining: &mut usize) {
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        let Some(message) = message.as_object_mut() else {
            continue;
        };
        let supported = !matches!(
            message.get("role").and_then(Value::as_str),
            Some("function") | None
        );
        let Some(content) = message.get_mut("content") else {
            continue;
        };
        if supported && let Some(text) = take_string(content, remaining) {
            *content = Value::Array(vec![marked("text", "text", text)]);
        } else if let Value::Array(parts) = content {
            for part in parts {
                mark_part(part, true, remaining);
            }
        }
    }
}

fn responses(root: &mut Map<String, Value>, remaining: &mut usize) {
    let instruction_triggered = root
        .get_mut("instructions")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .map(|mut text| {
            let matched = super::super::cache::strip_magic(&mut text);
            (matched, text)
        });
    let instruction_triggered = match instruction_triggered {
        Some((true, text)) => {
            root.insert("instructions".into(), Value::String(text));
            true
        }
        Some((false, _)) | None => false,
    };
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
            if let Some(text) = take_string(value, remaining) {
                *value = marked("input_text", "text", text);
            } else {
                mark_part(value, false, remaining);
            }
        }
    }
    let Some(input) = root.get_mut("input") else {
        return;
    };
    if let Some(text) = take_string(input, remaining) {
        *input = Value::Array(vec![message("user", text, true)]);
        return;
    }
    let Value::Array(items) = input else {
        return;
    };
    for item in items {
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        let role = item.get("role").and_then(Value::as_str).map(str::to_owned);
        for field in ["content", "output"] {
            let Some(content) = item.get_mut(field) else {
                continue;
            };
            if let Some(text) = take_string(content, remaining) {
                let kind = if role.as_deref() == Some("assistant") {
                    "output_text"
                } else {
                    "input_text"
                };
                *content = Value::Array(vec![marked(kind, "text", text)]);
            } else if let Value::Array(parts) = content {
                for part in parts {
                    mark_part(part, false, remaining);
                }
            }
        }
    }
}

fn take_string(value: &mut Value, remaining: &mut usize) -> Option<String> {
    let Value::String(text) = value else {
        return None;
    };
    if !super::super::cache::strip_magic(text) || text.trim().is_empty() || *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    Some(std::mem::take(text))
}

fn mark_part(part: &mut Value, chat: bool, remaining: &mut usize) {
    let Some(part) = part.as_object_mut() else {
        return;
    };
    let text_key = match (chat, part.get("type").and_then(Value::as_str)) {
        (true, Some("text")) => "text",
        (true, Some("refusal")) => "refusal",
        (false, Some("input_text" | "output_text")) => "text",
        (false, Some("refusal")) => "refusal",
        _ => return,
    };
    let already_marked = part.contains_key("prompt_cache_breakpoint");
    let Some(text) = part
        .get_mut(text_key)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
    else {
        return;
    };
    let mut text = text;
    if !super::super::cache::strip_magic(&mut text) {
        return;
    }
    part.insert(text_key.into(), Value::String(text.clone()));
    if !text.trim().is_empty() && *remaining > 0 && !already_marked {
        part.insert("prompt_cache_breakpoint".into(), breakpoint());
        *remaining -= 1;
    }
}

fn prepend_instruction_anchor(root: &mut Map<String, Value>) -> bool {
    let mut items = match root.remove("input") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(text)) => vec![message("user", text, false)],
        Some(Value::Array(items)) => items,
        Some(other) => {
            root.insert("input".into(), other);
            return false;
        }
    };
    items.insert(0, message("developer", " ".into(), true));
    root.insert("input".into(), Value::Array(items));
    true
}

fn message(role: &str, text: String, marked_part: bool) -> Value {
    let mut part =
        json!({"type":if role == "assistant" {"output_text"} else {"input_text"},"text":text});
    if marked_part {
        part["prompt_cache_breakpoint"] = breakpoint();
    }
    json!({"type":"message","role":role,"content":[part]})
}

fn marked(kind: &str, text_key: &str, text: String) -> Value {
    let mut value = json!({"type":kind,"prompt_cache_breakpoint":breakpoint()});
    value[text_key] = Value::String(text);
    value
}

fn breakpoint() -> Value {
    json!({"mode":"explicit"})
}
