use bytes::Bytes;
use http::HeaderMap;
use serde_json::{Value, json};

use super::{compact, converse, is_count_tokens, is_models, models};
use crate::channel::ShapeCtx;
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{claude_cache_control, claude_magic_cache};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind};

pub(super) fn request(body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
    if is_count_tokens(ctx.op) {
        let Ok(value) = serde_json::from_slice::<Value>(&body) else {
            return body;
        };
        let mut value = converse::request_value(value);
        if let Some(root) = value.as_object_mut() {
            root.retain(|key, _| {
                matches!(
                    key.as_str(),
                    "messages" | "system" | "toolConfig" | "additionalModelRequestFields"
                )
            });
        }
        return Bytes::from(json!({ "input": { "converse": value } }).to_string());
    }
    if ctx.op.kind() != OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) {
        return body;
    }
    let settings = RequestShapeSettings::from_value(ctx.settings);
    let body = if settings.enable_openai_magic_cache || settings.enable_claude_magic_cache {
        crate::channel::shaping::with_json_body(body, |value| {
            claude_magic_cache::apply_magic_string_cache_control_triggers(value);
            claude_cache_control::sanitize_claude_body(value);
        })
    } else {
        body
    };
    if compact::is_request(&body) {
        compact::request(body)
    } else {
        converse::request(body)
    }
}

pub(super) fn response(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    if !ctx.status.is_success() {
        return body;
    }
    if is_models(ctx.op) {
        return models::response(body, ctx.op.operation() == Operation::GetModel);
    }
    if is_count_tokens(ctx.op) {
        return crate::channel::shaping::with_json_body(body, |value| {
            let Some(root) = value.as_object_mut() else {
                return;
            };
            if let Some(tokens) = root.remove("inputTokens") {
                root.insert("input_tokens".into(), tokens);
            }
        });
    }
    if ctx.op.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) {
        converse::response(body)
    } else {
        body
    }
}
