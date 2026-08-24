use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Value, json};

pub(super) struct WebRequest {
    pub body: Value,
    pub uploads: Vec<Upload>,
    pub extended: bool,
    pub input_tokens: u64,
    pub model: String,
}

pub(super) struct Upload {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub file_name: String,
}

pub(super) fn parse(body: &Bytes) -> Result<Value, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("ClaudeWeb request JSON: {error}")))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(ChannelError::Prepare(
            "ClaudeWeb request must be an object".into(),
        ))
    }
}

pub(super) fn build(
    request: &Value,
    model: &str,
    prompt: &str,
    timezone: &str,
) -> Result<WebRequest, ChannelError> {
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .filter(|messages| !messages.is_empty())
        .ok_or_else(|| ChannelError::Prepare("ClaudeWeb messages missing".into()))?;
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
        return Err(ChannelError::Prepare(
            "ClaudeWeb request has no usable content".into(),
        ));
    }
    let explicit = request
        .pointer("/thinking/type")
        .and_then(Value::as_str)
        .is_some_and(|kind| matches!(kind, "enabled" | "adaptive"));
    let (model, suffix) = model
        .strip_suffix("-thinking")
        .map_or((model, false), |model| (model, true));
    let attachment = json!({
        "extracted_content":merged,
        "file_name":"paste.txt",
        "file_type":"txt",
        "file_size":merged.len(),
    });
    let input_tokens = u64::try_from(request.to_string().chars().count())
        .unwrap_or(u64::MAX)
        .div_ceil(4)
        .max(1);
    Ok(WebRequest {
        body: json!({
            "max_tokens_to_sample":request.get("max_tokens").and_then(Value::as_u64).unwrap_or(8192),
            "attachments":[attachment],
            "files":[],
            "model":model,
            "rendering_mode":"messages",
            "prompt":prompt,
            "timezone":timezone,
            "locale":"en-US",
            "effort":"medium",
            "thinking_mode":if explicit||suffix{"auto"}else{"off"},
            "tools":request.get("tools").cloned().unwrap_or_else(||json!([])),
            "turn_message_uuids":{
                "human_message_uuid":super::id::uuid(),
                "assistant_message_uuid":super::id::uuid(),
            }
        }),
        uploads,
        extended: explicit || suffix,
        input_tokens,
        model: model.into(),
    })
}

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
                block["content"] = json!([{"type":"text","text":text}]);
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
                        super::media::collect_image(block.get("source"), uploads)?;
                        parts.push("(image attached)".into());
                    }
                    _ => {}
                }
            }
            Ok(parts.join("\n"))
        }
        Some(Value::Object(_)) => {
            content_text(Some(&Value::Array(vec![value.cloned().unwrap()])), uploads)
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
