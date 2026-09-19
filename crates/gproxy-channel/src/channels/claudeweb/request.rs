//! Claude Messages request -> claude.ai completion body (v3
//! `claudeweb/request.rs` and `media.rs`). The web front end takes one
//! flattened prompt, so system and history collapse into `Human:` /
//! `Assistant:` turns; images become uploads referenced by `files`.

use base64::Engine as _;
use serde_json::{Value, json};

use super::bad_request;
use crate::channel::ChannelError;

pub(super) struct WebRequest {
    /// The completion body minus `files`, filled in after uploads.
    pub body: Value,
    pub uploads: Vec<Upload>,
    /// Extended thinking requested (`thinking.type` or a `-thinking` model).
    pub extended: bool,
    /// v3's estimate: request characters / 4; claude.ai reports no usage.
    pub input_tokens: u64,
    pub model: String,
}

pub(super) struct Upload {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub file_name: String,
}

pub(super) fn parse(body: &[u8]) -> Result<Value, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| bad_request(format!("Claude Web request JSON: {error}")))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(bad_request("Claude Web request must be an object"))
    }
}

pub(super) fn build(
    request: &Value,
    prompt: &str,
    timezone: &str,
) -> Result<WebRequest, ChannelError> {
    let model = request
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| bad_request("Claude Web request has no model"))?;
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .filter(|messages| !messages.is_empty())
        .ok_or_else(|| bad_request("Claude Web messages missing"))?;
    let mut uploads = Vec::new();
    let mut merged = content_text(request.get("system"), &mut uploads)?;
    for message in messages {
        let content = content_text(message.get("content"), &mut uploads)?;
        if content.trim().is_empty() {
            continue;
        }
        if !merged.is_empty() {
            let role = if message.get("role").and_then(Value::as_str) == Some("assistant") {
                "Assistant"
            } else {
                "Human"
            };
            merged.push_str(&format!("\n\n{role}: "));
        }
        merged.push_str(content.trim());
    }
    if merged.trim().is_empty() {
        return Err(bad_request("Claude Web request has no usable content"));
    }
    let explicit = request
        .pointer("/thinking/type")
        .and_then(Value::as_str)
        .is_some_and(|kind| matches!(kind, "enabled" | "adaptive"));
    let (model, suffix) = model
        .strip_suffix("-thinking")
        .map_or((model, false), |model| (model, true));
    let prompt = if prompt.trim().is_empty() {
        merged
    } else {
        format!("{}\n\n{}", prompt.trim(), merged)
    };
    let input_tokens = estimate_tokens(&request.to_string()).max(1);
    Ok(WebRequest {
        body: json!({
            "max_tokens_to_sample": request.get("max_tokens").and_then(Value::as_u64).unwrap_or(8192),
            "attachments": [],
            "files": [],
            "model": model,
            "rendering_mode": "messages",
            "prompt": prompt,
            "timezone": timezone,
            "locale": "en-US",
            "effort": "medium",
            "thinking_mode": if explicit || suffix { "auto" } else { "off" },
            "tools": web_tools(request),
            "turn_message_uuids": {
                "human_message_uuid": super::id::uuid()?,
                "assistant_message_uuid": super::id::uuid()?,
            }
        }),
        uploads,
        extended: explicit || suffix,
        input_tokens,
        model: model.to_owned(),
    })
}

/// v3's character estimate, used for both the prompt and the answer.
pub(super) fn estimate_tokens(text: &str) -> u64 {
    u64::try_from(text.chars().count())
        .unwrap_or(u64::MAX)
        .div_ceil(4)
}

/// Client tools go through as-is except that Messages' `type: custom` is
/// not part of the web tool schema.
fn web_tools(request: &Value) -> Value {
    let mut tools = request
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for tool in &mut tools {
        let Some(tool) = tool.as_object_mut() else {
            continue;
        };
        if tool.get("type").and_then(Value::as_str) == Some("custom") {
            tool.remove("type");
        }
    }
    Value::Array(tools)
}

/// The `tool_result` blocks of the final user message, each with a string
/// `content` promoted to a text block as `/tool_result` expects.
pub(super) fn tool_results(request: &Value) -> Vec<Value> {
    request
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.last())
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
        .cloned()
        .map(|mut block| {
            if let Some(text) = block.get("content").and_then(Value::as_str) {
                block["content"] = json!([{"type": "text", "text": text}]);
            }
            block
        })
        .collect()
}

fn content_text(value: Option<&Value>, uploads: &mut Vec<Upload>) -> Result<String, ChannelError> {
    match value {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => push_text(&mut parts, block.get("text")),
                    Some("thinking") => push_text(&mut parts, block.get("thinking")),
                    Some("tool_use" | "server_tool_use") => parts.push(tool_use(block)),
                    Some("tool_result") => parts.push(format!(
                        "<function_results>{}</function_results>",
                        content_text(block.get("content"), uploads)?
                    )),
                    Some("image") => {
                        collect_image(block.get("source"), uploads)?;
                        parts.push("(image attached)".into());
                    }
                    _ => {}
                }
            }
            Ok(parts.join("\n"))
        }
        Some(block @ Value::Object(_)) => {
            content_text(Some(&Value::Array(vec![block.clone()])), uploads)
        }
        _ => Ok(String::new()),
    }
}

fn push_text(parts: &mut Vec<String>, value: Option<&Value>) {
    if let Some(text) = value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        parts.push(text.into());
    }
}

fn tool_use(block: &Value) -> String {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
    format!("<function_calls><invoke name=\"{name}\">{input}</invoke></function_calls>")
}

/// Base64 image sources (or data URLs) become multipart uploads.
fn collect_image(source: Option<&Value>, uploads: &mut Vec<Upload>) -> Result<(), ChannelError> {
    let source = source.ok_or_else(|| bad_request("image source missing"))?;
    let media = source
        .get("media_type")
        .and_then(Value::as_str)
        .unwrap_or("image/png");
    let data = source
        .get("data")
        .and_then(Value::as_str)
        .or_else(|| {
            source
                .get("url")
                .and_then(Value::as_str)?
                .split_once(',')
                .map(|(_, data)| data)
        })
        .ok_or_else(|| bad_request("image must be a base64 data URL"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| bad_request(format!("image base64: {error}")))?;
    let extension = media
        .split('/')
        .nth(1)
        .unwrap_or("png")
        .split(';')
        .next()
        .unwrap_or("png");
    uploads.push(Upload {
        bytes,
        media_type: media.into(),
        file_name: format!("image.{extension}"),
    });
    Ok(())
}
