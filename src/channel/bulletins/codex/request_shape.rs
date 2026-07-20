//! Responses request-body shaping for the ChatGPT Codex backend.

use bytes::Bytes;
use serde_json::{Value, json};

use crate::channel::ShapeCtx;
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{self, openai_cache};

const STRIP_KEYS: &[&str] = &[
    "max_output_tokens",
    "metadata",
    "stream_options",
    "temperature",
    "top_p",
    "top_logprobs",
    "safety_identifier",
    "truncation",
];

pub(super) fn shape(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    let settings = RequestShapeSettings::from_value(ctx.settings);
    let body = match settings
        .enable_magic_cache
        .then(|| openai_cache::kind_for_operation(ctx.op))
        .flatten()
    {
        Some(kind) => shaping::with_json_body(body, |value| {
            openai_cache::apply_magic_string_cache_breakpoints(value, kind)
        }),
        None => body,
    };
    normalize_responses_body(&body)
}

/// Force streaming/non-persistence, drop unsupported sampling fields, and lift
/// system input messages into top-level instructions. Invalid JSON is unchanged.
fn normalize_responses_body(body: &Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.clone();
    };
    let Some(object) = value.as_object_mut() else {
        return body.clone();
    };

    object.insert("stream".into(), Value::Bool(true));
    object.insert("store".into(), Value::Bool(false));
    for key in STRIP_KEYS {
        object.remove(*key);
    }

    let mut instructions = object
        .get("instructions")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some(text) = object.get("input").and_then(Value::as_str) {
        let text = text.to_string();
        object.insert(
            "input".into(),
            json!([{ "type": "message", "role": "user", "content": text }]),
        );
    }
    if let Some(Value::Array(items)) = object.get_mut("input") {
        let mut retained = Vec::with_capacity(items.len());
        for item in std::mem::take(items) {
            if is_system_role(&item) {
                append_instruction(&mut instructions, item_text(&item));
            } else {
                retained.push(item);
            }
        }
        *items = retained;
    }

    object.insert("instructions".into(), Value::String(instructions));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .unwrap_or_else(|_| body.clone())
}

fn is_system_role(item: &Value) -> bool {
    item.get("role")
        .and_then(Value::as_str)
        .is_some_and(|role| role.eq_ignore_ascii_case("system"))
}

fn append_instruction(instructions: &mut String, text: String) {
    if text.is_empty() {
        return;
    }
    if !instructions.is_empty() {
        instructions.push('\n');
    }
    instructions.push_str(&text);
}

fn item_text(item: &Value) -> String {
    let mut parts = Vec::new();
    collect_text(item.get("content").unwrap_or(&Value::Null), &mut parts);
    parts.join("\n")
}

fn collect_text(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::String(text) if !text.is_empty() => parts.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_text(item, parts)),
        Value::Object(object) => {
            if let Some(text) = object.get("text").and_then(Value::as_str)
                && !text.is_empty()
            {
                parts.push(text.to_string());
            }
        }
        _ => {}
    }
}
