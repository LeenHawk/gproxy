//! Upgrade legacy native Messages/count-tokens requests before upstream dispatch.
//! Work on the original JSON so provider extensions and signed blocks survive;
//! only fields rejected by API 2026-10-21 are changed.

use crate::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use serde_json::Value;

pub fn applies_to(operation: OperationKey) -> bool {
    operation.dialect == Dialect::Claude
        && matches!(
            operation.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent | Operation::CountTokens
        )
}

/// Requires a buffered body. Unchanged requests retain their exact bytes.
/// Fallback credits bind the original payload and must not be rewritten.
pub fn prepare(operation: OperationKey, request: &mut WireRequest<HttpBody>) {
    if !applies_to(operation) {
        return;
    }
    let HttpBody::Bytes(bytes) = &request.body else {
        return;
    };
    let Ok(mut body) = serde_json::from_slice::<Value>(bytes) else {
        return;
    };
    let Some(root) = body.as_object_mut() else {
        return;
    };
    if root
        .get("fallback_credit_token")
        .is_some_and(|value| !value.is_null())
    {
        return;
    }
    let mut changed = false;
    for field in ["temperature", "top_p", "top_k"] {
        changed |= root.remove(field).is_some();
    }
    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        let mut output = Vec::with_capacity(messages.len());
        for mut message in messages.drain(..) {
            let role = message.get("role").and_then(Value::as_str);
            let user = role == Some("user");
            let assistant = role == Some("assistant");
            if let Some(blocks) = message.get_mut("content").and_then(Value::as_array_mut) {
                if user {
                    for block in blocks.iter_mut() {
                        changed |= user_block(block);
                    }
                }
                if assistant && blocks.iter().any(is_document) {
                    let blocks = std::mem::take(blocks);
                    for mut block in blocks {
                        let mut part = message.clone();
                        if is_document(&block) {
                            user_block(&mut block);
                            part["role"] = Value::String("user".into());
                        }
                        part["content"] = Value::Array(vec![block]);
                        output.push(part);
                    }
                    changed = true;
                    continue;
                }
            }
            output.push(message);
        }
        *messages = output;
    }
    if changed && let Ok(bytes) = serde_json::to_vec(&body) {
        request.headers.remove(http::header::CONTENT_LENGTH);
        request.body = HttpBody::Bytes(Bytes::from(bytes));
    }
}

fn is_document(block: &Value) -> bool {
    block.get("type").and_then(Value::as_str) == Some("document")
}

fn user_block(block: &mut Value) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => {
            if block
                .get("citations")
                .and_then(Value::as_array)
                .is_some_and(|values| !values.is_empty())
            {
                block
                    .as_object_mut()
                    .expect("text block is an object")
                    .remove("citations");
                true
            } else {
                false
            }
        }
        Some("search_result" | "tool_result" | "mcp_tool_result") => {
            let mut changed = false;
            if let Some(blocks) = block.get_mut("content").and_then(Value::as_array_mut) {
                for block in blocks {
                    changed |= user_block(block);
                }
            }
            changed
        }
        Some("document") => {
            let mut changed = false;
            if block.pointer("/source/type").and_then(Value::as_str) == Some("content")
                && let Some(blocks) = block
                    .pointer_mut("/source/content")
                    .and_then(Value::as_array_mut)
            {
                for block in blocks {
                    changed |= user_block(block);
                }
            }
            changed
        }
        Some("web_fetch_tool_result" | "web_fetch_result") => {
            block.get_mut("content").is_some_and(user_block)
        }
        _ => false,
    }
}
