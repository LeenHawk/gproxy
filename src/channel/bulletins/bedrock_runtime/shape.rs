use base64::Engine;
use bytes::Bytes;
use http::HeaderMap;
use serde_json::{Value, json};

use crate::channel::ShapeCtx;
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{
    self, claude_cache_control, claude_fallback, claude_magic_cache, openai_cache,
};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

const COMPACTION_BETA: &str = "compact-2026-01-12";
const FALLBACK_BETA: &str = "server-side-fallback-2026-06-01";

pub(super) fn request(body: Bytes, _headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
    if is_claude_count_tokens(ctx) {
        return count_tokens_request(body);
    }
    let settings = RequestShapeSettings::from_value(ctx.settings);
    if let Some(kind) = openai_cache::kind_for_operation(ctx.op) {
        if !settings.enable_magic_cache {
            return body;
        }
        return shaping::with_json_body(body, |value| {
            openai_cache::apply_magic_string_cache_breakpoints(value, kind)
        });
    }
    if !is_claude_messages(ctx) {
        return body;
    }

    shaping::with_json_body(body, |value| {
        if settings.enable_magic_cache {
            claude_magic_cache::apply_magic_string_cache_control_triggers(value);
        }
        claude_cache_control::sanitize_claude_body(value);
        let fallback = settings.enable_claude_fable_fallback
            && claude_fallback::apply_fable_to_opus48_body_only(value);
        let compaction = has_compaction_edit(value);
        let Some(root) = value.as_object_mut() else {
            return;
        };
        root.insert(
            "anthropic_version".into(),
            Value::String("bedrock-2023-05-31".into()),
        );
        root.remove("model");
        root.remove("stream");
        if compaction {
            append_beta(root, COMPACTION_BETA);
        }
        if fallback {
            append_beta(root, FALLBACK_BETA);
        }
    })
}

pub(super) fn response(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    if !is_claude_count_tokens(ctx) {
        return body;
    }
    shaping::with_json_body(body, |value| {
        let Some(root) = value.as_object_mut() else {
            return;
        };
        if let Some(tokens) = root.remove("inputTokens") {
            root.insert("input_tokens".into(), tokens);
        }
    })
}

fn count_tokens_request(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(root) = value.as_object_mut() else {
        return body;
    };
    root.remove("model");
    root.insert(
        "anthropic_version".into(),
        Value::String("bedrock-2023-05-31".into()),
    );
    let Ok(invoke_body) = serde_json::to_vec(&value) else {
        return body;
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(invoke_body);
    Bytes::from(json!({ "input": { "invokeModel": { "body": encoded } } }).to_string())
}

fn is_claude_messages(ctx: &ShapeCtx) -> bool {
    ctx.op.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}

fn is_claude_count_tokens(ctx: &ShapeCtx) -> bool {
    ctx.op.operation == Operation::CountTokens
        && ctx.op.kind == OperationKind::Provider(Provider::Claude)
}

fn has_compaction_edit(body: &Value) -> bool {
    body.pointer("/context_management/edits")
        .and_then(Value::as_array)
        .is_some_and(|edits| {
            edits
                .iter()
                .any(|edit| edit.get("type").and_then(Value::as_str) == Some("compact_20260112"))
        })
}

fn append_beta(root: &mut serde_json::Map<String, Value>, token: &str) {
    let beta = root.entry("anthropic_beta").or_insert_with(|| json!([]));
    let Some(values) = beta.as_array_mut() else {
        return;
    };
    if !values.iter().any(|value| value.as_str() == Some(token)) {
        values.push(Value::String(token.to_owned()));
    }
}
