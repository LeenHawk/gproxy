//! OpenAI Responses → Kiro Smithy request builder.
//!
//! Kiro speaks neither OpenAI nor Claude on the wire: chat goes through the
//! Smithy REST-JSON `POST /generateAssistantResponse`, whose body is a
//! `conversationState` graph. [`build_request_body`] converts an inbound OpenAI
//! Responses body into that shape:
//!
//! ```text
//!   { conversationState: {
//!       conversationId, chatTriggerType:"MANUAL", agentContinuationId,
//!       history: [ {userInputMessage|assistantResponseMessage} ... ],
//!       currentMessage: { userInputMessage: { content, modelId, origin,
//!                          userInputMessageContext{ editorState, tools, images } } } },
//!     profileArn?, inferenceConfig? }
//! ```
//!
//! Ported from the v1 `gproxy-channel` kiro impl. System / developer messages
//! are folded into a "system priming" user→assistant turn pair (Kiro has no
//! system role). The LAST user turn becomes `currentMessage`; everything before
//! it is `history`. Tools, images, and inference params are mapped best-effort
//! (see the per-helper notes). The `profileArn` is lifted to the TOP level by
//! the caller ([`super::KiroChannel::prepare`]).

use serde_json::{Map, Value, json};

use crate::channel::ChannelError;

use super::request_tools;

const DEFAULT_ORIGIN: &str = super::ORIGIN;
const DEFAULT_AGENT_TASK_TYPE: &str = "vibe";

/// Build the Kiro `conversationState` request body from an OpenAI Responses
/// body. `model` is the upstream-mapped model id; `conversation_id` is a fresh
/// per-call id (caller supplies a UUID — kept out of this module so it stays
/// wasm-pure). Returns the serialized JSON bytes ready to POST.
pub(super) fn build_request_body(
    responses_body: &[u8],
    model: &str,
    conversation_id: &str,
) -> Result<Vec<u8>, ChannelError> {
    let value: Value = serde_json::from_slice(responses_body)
        .map_err(|e| ChannelError::Build(format!("kiro request body is not JSON: {e}")))?;

    let conversation_state = conversation_state(&value, model, conversation_id)?;
    let mut request = json!({ "conversationState": conversation_state });
    if let Some(inference) = inference_config(&value)
        && let Some(obj) = request.as_object_mut()
    {
        obj.insert("inferenceConfig".into(), inference);
    }
    serde_json::to_vec(&request)
        .map_err(|e| ChannelError::Build(format!("serialize kiro request: {e}")))
}

/// Map a model id to Kiro's accepted set (dot-versioned Claude ids). Unknown ids
/// pass through. Mirrors v1's substring table (only the entries that differ from
/// the input matter; the longest needle wins by ordering).
pub(super) fn map_model(model: &str) -> String {
    let lower = model.to_ascii_lowercase().replace('_', "-");
    for (needle, replacement) in [
        ("claude-sonnet-4-20250514", "claude-sonnet-4"),
        ("claude-sonnet-4-5", "claude-sonnet-4.5"),
        ("claude-sonnet-4-6", "claude-sonnet-4.6"),
        ("claude-opus-4-7", "claude-opus-4.7"),
        ("claude-haiku-4-5", "claude-haiku-4.5"),
        ("claude-opus-4-5", "claude-opus-4.5"),
        ("claude-opus-4-6", "claude-opus-4.6"),
        ("claude-3-5-sonnet", "claude-sonnet-4.5"),
        ("claude-3-opus", "claude-sonnet-4.5"),
        ("claude-3-sonnet", "claude-sonnet-4"),
        ("claude-3-haiku", "claude-haiku-4.5"),
        ("gpt-4-turbo", "claude-sonnet-4.5"),
        ("gpt-4o", "claude-sonnet-4.5"),
        ("gpt-4", "claude-sonnet-4.5"),
        ("gpt-3.5-turbo", "claude-sonnet-4.5"),
    ] {
        if lower.contains(needle) {
            return replacement.to_string();
        }
    }
    model.to_string()
}

/// Assemble the `conversationState`: walk the OpenAI `input` (string / array /
/// object) into Kiro messages, fold system priming, split off the final user
/// turn as `currentMessage`, and attach tools to it.
fn conversation_state(
    value: &Value,
    model: &str,
    conversation_id: &str,
) -> Result<Value, ChannelError> {
    let mut messages: Vec<Value> = Vec::new();
    let instructions = optional_text(value.get("instructions"));
    let model = map_model(model);
    let input = value.get("input").ok_or_else(|| {
        ChannelError::Build("kiro request requires OpenAI Responses `input`".into())
    })?;

    match input {
        Value::String(text) => {
            if let Some(system) = instructions.as_deref() {
                push_system_priming(&mut messages, system, &model);
            }
            messages.push(user_message(
                fallback_content(text, false),
                &model,
                Vec::new(),
            ));
        }
        Value::Array(items) => {
            let mut pending_system = instructions.unwrap_or_default();
            let mut pending_results: Vec<Value> = Vec::new();
            for item in items {
                // Responses tool items fold into Kiro's shape: `function_call` →
                // the assistant turn's toolUses; `function_call_output` → a tool
                // result on the next user turn.
                let mut fc = false;
                match item.get("type").and_then(Value::as_str) {
                    Some("function_call") => fc = true,
                    Some("function_call_output") => {
                        pending_results.push(request_tools::tool_result_entry(item));
                        continue;
                    }
                    _ => {}
                }
                if fc {
                    request_tools::flush_tool_results(&mut messages, &mut pending_results, &model);
                    request_tools::append_tool_use(&mut messages, item);
                    continue;
                }
                let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
                let (text, images) = text_and_images(item.get("content").unwrap_or(item))?;
                if matches!(role, "system" | "developer") {
                    pending_system = join_nonempty(&[&pending_system, &text]);
                    continue;
                }
                if !pending_system.is_empty() && role == "user" {
                    push_system_priming(&mut messages, &pending_system, &model);
                    pending_system.clear();
                }
                match role {
                    "assistant" => {
                        // tool results precede the assistant turn → own user turn
                        request_tools::flush_tool_results(
                            &mut messages,
                            &mut pending_results,
                            &model,
                        );
                        messages.push(assistant_message(text));
                    }
                    "user" => {
                        let content = fallback_content(&text, !images.is_empty());
                        let mut msg = user_message(content, &model, images);
                        if !pending_results.is_empty() {
                            request_tools::attach_tool_results(
                                &mut msg,
                                std::mem::take(&mut pending_results),
                            );
                        }
                        messages.push(msg);
                    }
                    other => {
                        return Err(ChannelError::Build(format!(
                            "kiro does not support OpenAI role '{other}'"
                        )));
                    }
                }
            }
            if !pending_system.is_empty() {
                push_system_priming(&mut messages, &pending_system, &model);
            }
            // Trailing tool results (a "continue after tool execution" request)
            // become the current user turn.
            request_tools::flush_tool_results(&mut messages, &mut pending_results, &model);
        }
        Value::Object(_) => {
            let (text, images) = text_and_images(input.get("content").unwrap_or(input))?;
            if let Some(system) = instructions.as_deref() {
                push_system_priming(&mut messages, system, &model);
            }
            messages.push(user_message(
                fallback_content(&text, !images.is_empty()),
                &model,
                images,
            ));
        }
        _ => {
            return Err(ChannelError::Build(
                "kiro `input` must be text/array/object".into(),
            ));
        }
    }

    let mut current = messages
        .pop()
        .ok_or_else(|| ChannelError::Build("kiro request produced no messages".into()))?;
    if current.get("userInputMessage").is_none() {
        return Err(ChannelError::Build(
            "kiro request requires the final message to be a user message".into(),
        ));
    }
    let tools = request_tools::tools_from(value.get("tools"))?;
    if !tools.is_empty() {
        request_tools::insert_tools(&mut current, tools);
    }

    Ok(json!({
        "conversationId": conversation_id,
        "history": messages,
        "currentMessage": current,
        "chatTriggerType": "MANUAL",
        "agentTaskType": DEFAULT_AGENT_TASK_TYPE,
    }))
}

/// Build a `userInputMessage` wrapper carrying content, model, origin, an empty
/// editor state, and any image blocks.
pub(super) fn user_message(content: String, model: &str, images: Vec<Value>) -> Value {
    let mut message = json!({
        "origin": DEFAULT_ORIGIN,
        "content": content,
        "userInputMessageContext": { "editorState": {} }
    });
    if let Some(obj) = message.as_object_mut() {
        if !model.trim().is_empty() {
            obj.insert("modelId".into(), Value::String(model.to_string()));
        }
        if !images.is_empty() {
            obj.insert("images".into(), Value::Array(images));
        }
    }
    json!({ "userInputMessage": message })
}

/// Build an `assistantResponseMessage` wrapper.
pub(super) fn assistant_message(content: String) -> Value {
    json!({ "assistantResponseMessage": { "content": content } })
}

/// Fold a system/developer prompt into a user→assistant priming pair (Kiro has
/// no system role; this is v1's accepted workaround).
fn push_system_priming(messages: &mut Vec<Value>, system: &str, model: &str) {
    let system = system.trim();
    if system.is_empty() {
        return;
    }
    messages.push(user_message(system.to_string(), model, Vec::new()));
    messages.push(assistant_message(
        "I will follow these instructions.".into(),
    ));
}

/// Non-empty user content, or a placeholder (Kiro rejects empty content).
pub(super) fn fallback_content(text: &str, has_images: bool) -> String {
    let text = text.trim();
    if !text.is_empty() {
        text.to_string()
    } else if has_images {
        "Please analyze the attached image.".to_string()
    } else {
        ".".to_string()
    }
}

/// Newline-join the non-empty trimmed parts (used to merge system prompts).
fn join_nonempty(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Extract a plain-text string from an optional Responses value (instructions).
fn optional_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Null => None,
        Value::String(text) => (!text.is_empty()).then(|| text.clone()),
        other => text_and_images(other).ok().map(|(text, _)| text),
    }
}

/// Flatten Responses content into `(text, images)`. Text parts are newline
/// joined; image parts become Kiro image blocks (data URLs only). Best-effort:
/// unknown scalar content yields empty text.
fn text_and_images(value: &Value) -> Result<(String, Vec<Value>), ChannelError> {
    match value {
        Value::String(text) => Ok((text.clone(), Vec::new())),
        Value::Array(items) => {
            let mut text_parts = Vec::new();
            let mut images = Vec::new();
            for item in items {
                let (text, mut item_images) = text_and_images(item)?;
                if !text.is_empty() {
                    text_parts.push(text);
                }
                images.append(&mut item_images);
            }
            Ok((text_parts.join("\n"), images))
        }
        Value::Object(obj) => match obj.get("type").and_then(Value::as_str) {
            Some("input_text" | "text" | "output_text") => Ok((
                obj.get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                Vec::new(),
            )),
            Some("image_url" | "input_image") => {
                let url = obj
                    .get("image_url")
                    .and_then(|v| v.as_str().or_else(|| v.get("url").and_then(Value::as_str)))
                    .ok_or_else(|| {
                        ChannelError::Build("kiro image requires an image_url data URL".into())
                    })?;
                Ok((String::new(), vec![image_block(url)?]))
            }
            Some(other) => Err(ChannelError::Build(format!(
                "kiro does not support content part type '{other}'"
            ))),
            None => match obj.get("content") {
                Some(content) => text_and_images(content),
                None => Ok((String::new(), Vec::new())),
            },
        },
        Value::Null => Ok((String::new(), Vec::new())),
        _ => Err(ChannelError::Build(
            "kiro only supports text/image content conversion".into(),
        )),
    }
}

/// Build a Kiro image block `{format, source:{bytes}}` from a `data:image/...`
/// data URL (the only image source Kiro accepts).
fn image_block(data_url: &str) -> Result<Value, ChannelError> {
    let (meta, bytes) = data_url
        .split_once(',')
        .ok_or_else(|| ChannelError::Build("kiro image must be a data URL".into()))?;
    if !meta.starts_with("data:image/") {
        return Err(ChannelError::Build(
            "kiro image must be an image data URL".into(),
        ));
    }
    let format = meta
        .strip_prefix("data:image/")
        .and_then(|m| m.split(';').next())
        .filter(|m| !m.is_empty())
        .unwrap_or("jpeg")
        .to_ascii_lowercase();
    Ok(json!({ "format": format, "source": { "bytes": bytes } }))
}

/// Map Responses sampling fields to Kiro's `inferenceConfig` (camelCase).
fn inference_config(value: &Value) -> Option<Value> {
    let mut obj = Map::new();
    if let Some(max) = value.get("max_output_tokens").and_then(Value::as_u64) {
        obj.insert("maxTokens".into(), json!(max));
    }
    if let Some(temp) = value.get("temperature").and_then(Value::as_f64) {
        obj.insert("temperature".into(), json!(temp));
    }
    if let Some(top_p) = value.get("top_p").and_then(Value::as_f64) {
        obj.insert("topP".into(), json!(top_p));
    }
    (!obj.is_empty()).then_some(Value::Object(obj))
}
