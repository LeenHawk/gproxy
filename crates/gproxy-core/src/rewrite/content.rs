use gproxy_protocol::Dialect;
use serde_json::{Value, json};

use super::action::TextPosition;

pub(super) fn system_text(
    body: &mut Value,
    kind: Option<Dialect>,
    text: &str,
    position: TextPosition,
) -> bool {
    let Some(object) = body.as_object_mut() else {
        return false;
    };
    match kind {
        Some(Dialect::Claude) => inject_text(
            object.entry("system").or_insert(Value::Null),
            text,
            position,
        ),
        Some(Dialect::OpenAiChat) => {
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
                            message
                                .get("role")
                                .and_then(Value::as_str)
                                .is_some_and(|role| matches!(role, "system" | "developer"))
                        })
                        .count();
                    messages.insert(index, message);
                }
            }
            true
        }
        Some(Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket) => inject_text(
            object.entry("instructions").or_insert(Value::Null),
            text,
            position,
        ),
        Some(Dialect::Gemini) => {
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
        _ => false,
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
