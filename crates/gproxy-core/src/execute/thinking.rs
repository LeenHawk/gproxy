//! Provider-wide removal of carried thinking, before conversion and native sends.
//! Only protocol history fields are visited; tool arguments and generation
//! controls are application data, even when they contain reasoning-like keys.

use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use serde_json::Value;

pub(super) fn enabled(config: &Value, operation: OperationKey) -> bool {
    config.get("forward_thinking").and_then(Value::as_bool) == Some(false)
        && matches!(
            operation.operation,
            Operation::GenerateContent
                | Operation::StreamGenerateContent
                | Operation::CountTokens
                | Operation::CompactContent
                | Operation::CreateConversation
        )
}

pub(super) fn strip_request(dialect: Dialect, request: &mut WireRequest<HttpBody>) {
    if let HttpBody::Bytes(bytes) = &request.body
        && let Some(body) = strip_bytes(dialect, bytes)
    {
        request.body = HttpBody::Bytes(body.into());
        request.headers.remove(http::header::CONTENT_LENGTH);
    }
}

pub(super) fn strip_bytes(dialect: Dialect, bytes: &[u8]) -> Option<Vec<u8>> {
    let mut body: Value = serde_json::from_slice(bytes).ok()?;
    strip_value(dialect, &mut body).then(|| serde_json::to_vec(&body).expect("JSON value"))
}

fn strip_value(dialect: Dialect, body: &mut Value) -> bool {
    match dialect {
        Dialect::Claude | Dialect::OpenAiChat => {
            let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
                return false;
            };
            let mut changed = false;
            messages.retain_mut(|message| {
                if dialect == Dialect::OpenAiChat
                    && let Some(object) = message.as_object_mut()
                {
                    for key in ["reasoning", "reasoning_content", "reasoning_details"] {
                        changed |= object.remove(key).is_some();
                    }
                }
                if let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) {
                    let before = content.len();
                    content.retain(|block| !is_thinking(block));
                    let removed = before != content.len();
                    changed |= removed;
                    // Do not leave a thinking-only message with invalid empty content.
                    if removed && content.is_empty() {
                        return message.get("tool_calls").is_some()
                            || message.get("function_call").is_some();
                    }
                }
                true
            });
            changed
        }
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => {
            let mut changed = false;
            for key in ["input", "items"] {
                if let Some(items) = body.get_mut(key).and_then(Value::as_array_mut) {
                    let before = items.len();
                    items.retain(|item| !is_thinking(item));
                    changed |= before != items.len();
                }
            }
            changed
        }
        Dialect::Gemini => {
            // countTokens can wrap its history in generateContentRequest.
            let mut changed = false;
            for key in ["generateContentRequest", "generate_content_request"] {
                changed |= body
                    .get_mut(key)
                    .is_some_and(|request| strip_value(dialect, request));
            }
            if let Some(contents) = body.get_mut("contents").and_then(Value::as_array_mut) {
                contents.retain_mut(|content| {
                    let Some(parts) = content.get_mut("parts").and_then(Value::as_array_mut) else {
                        return true;
                    };
                    let before = parts.len();
                    parts.retain_mut(|part| {
                        if part.get("thought").and_then(Value::as_bool) == Some(true) {
                            changed = true;
                            return false;
                        }
                        if let Some(object) = part.as_object_mut() {
                            let signature = object.remove("thoughtSignature").is_some()
                                | object.remove("thought_signature").is_some();
                            changed |= signature;
                            if signature {
                                object.remove("thought");
                                return !object.is_empty();
                            }
                        }
                        true
                    });
                    parts.len() == before || !parts.is_empty()
                });
            }
            changed
        }
    }
}

fn is_thinking(block: &Value) -> bool {
    matches!(
        block.get("type").and_then(Value::as_str),
        Some("thinking" | "redacted_thinking" | "reasoning")
    )
}
