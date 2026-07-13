//! Anthropic Messages request -> claude.ai web completion request.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde_json::{Value, json};

use crate::channel::ChannelError;

pub(super) struct WebRequest {
    pub body: Value,
    pub uploads: Vec<Upload>,
    pub extended_thinking: bool,
    pub input_tokens: u64,
    pub model: String,
}

pub(super) struct Upload {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub file_name: String,
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
        .ok_or_else(|| ChannelError::Build("claudeweb request has no messages array".into()))?;
    if messages.is_empty() {
        return Err(ChannelError::Build(
            "claudeweb request messages are empty".into(),
        ));
    }

    let mut uploads = Vec::new();
    let merged = merge_messages(request.get("system"), messages, &mut uploads)?;
    if merged.trim().is_empty() {
        return Err(ChannelError::Build(
            "claudeweb request has no usable message content".into(),
        ));
    }
    let max_tokens = request
        .get("max_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(8192);
    let tools = request
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let explicit_thinking = request
        .get("thinking")
        .and_then(|v| v.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| matches!(kind, "enabled" | "adaptive"));
    let (model, suffix_thinking) = model
        .strip_suffix("-thinking")
        .map_or((model, false), |model| (model, true));
    let extended_thinking = explicit_thinking || suffix_thinking;
    let input_tokens = estimate_value_tokens(request);

    let merged_len = merged.len();
    let attachment = json!({
        "extracted_content": merged,
        "file_name": "paste.txt",
        "file_type": "txt",
        "file_size": merged_len,
    });
    Ok(WebRequest {
        body: json!({
            "max_tokens_to_sample": max_tokens,
            "attachments": [attachment],
            "files": [],
            "model": model,
            "rendering_mode": "messages",
            "prompt": prompt,
            "timezone": timezone,
            "locale": "en-US",
            "effort": "medium",
            "thinking_mode": if extended_thinking { "auto" } else { "off" },
            "tools": tools,
            "turn_message_uuids": {
                "human_message_uuid": crate::util::rand::uuid_v7(),
                "assistant_message_uuid": crate::util::rand::uuid_v7(),
            },
        }),
        uploads,
        extended_thinking,
        input_tokens,
        model: model.to_owned(),
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
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
                .cloned()
                .map(|mut block| {
                    if let Some(text) = block.get("content").and_then(Value::as_str) {
                        block["content"] = json!([{"type":"text","text":text}]);
                    }
                    block
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn estimate_value_tokens(value: &Value) -> u64 {
    crate::tokenize::count_text(&value.to_string()).max(1)
}

fn merge_messages(
    system: Option<&Value>,
    messages: &[Value],
    uploads: &mut Vec<Upload>,
) -> Result<String, ChannelError> {
    let mut out = system_text(system);
    let mut emitted_message = false;
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let content = content_text(message.get("content"), uploads)?;
        if content.trim().is_empty() {
            continue;
        }
        if !out.trim().is_empty() || emitted_message {
            let label = if role == "assistant" {
                "Assistant"
            } else {
                "Human"
            };
            out.push_str("\n\n");
            out.push_str(label);
            out.push_str(": ");
        }
        out.push_str(content.trim());
        emitted_message = true;
    }
    Ok(out)
}

fn system_text(system: Option<&Value>) -> String {
    match system {
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn content_text(
    content: Option<&Value>,
    uploads: &mut Vec<Upload>,
) -> Result<String, ChannelError> {
    match content {
        Some(Value::String(text)) => Ok(text.to_owned()),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            parts.push(text.to_owned());
                        }
                    }
                    Some("thinking") => {
                        if let Some(text) = block.get("thinking").and_then(Value::as_str) {
                            parts.push(format!(
                                "<\u{8}antml:thinking>\n{text}\n</\u{8}antml:thinking>"
                            ));
                        }
                    }
                    Some("tool_use") | Some("server_tool_use") => {
                        parts.push(tool_use_text(block));
                    }
                    Some("tool_result") => {
                        let result = content_text(block.get("content"), uploads)?;
                        parts.push(format!("<function_results>{result}</function_results>"));
                    }
                    Some("image") => {
                        collect_image(block.get("source"), uploads)?;
                        parts.push("(image attached)".to_owned());
                    }
                    _ => {}
                }
            }
            Ok(parts.join("\n"))
        }
        _ => Ok(String::new()),
    }
}

fn tool_use_text(block: &Value) -> String {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let mut out = format!("<\u{8}antml:function_calls>\n<\u{8}antml:invoke name=\"{name}\">\n");
    if let Some(input) = block.get("input").and_then(Value::as_object) {
        for (key, value) in input {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            out.push_str(&format!(
                "<\u{8}antml:parameter name=\"{key}\">{value}</\u{8}antml:parameter>\n"
            ));
        }
    }
    out.push_str("</\u{8}antml:invoke>\n</\u{8}antml:function_calls>");
    out
}

fn collect_image(source: Option<&Value>, uploads: &mut Vec<Upload>) -> Result<(), ChannelError> {
    let Some(source) = source else {
        return Ok(());
    };
    if source.get("type").and_then(Value::as_str) == Some("file") {
        return Err(ChannelError::Build(
            "claudeweb does not accept an existing Anthropic file UUID".into(),
        ));
    }
    let media_type = source
        .get("media_type")
        .and_then(Value::as_str)
        .unwrap_or("image/png")
        .to_owned();
    let data = match source.get("type").and_then(Value::as_str) {
        Some("base64") => source.get("data").and_then(Value::as_str),
        Some("url") => source
            .get("url")
            .and_then(Value::as_str)
            .and_then(data_url_payload),
        _ => None,
    }
    .ok_or_else(|| {
        ChannelError::Build("claudeweb image input must be base64 or a base64 data URL".into())
    })?;
    let bytes = BASE64_STANDARD
        .decode(data)
        .map_err(|e| ChannelError::Build(format!("claudeweb image base64: {e}")))?;
    uploads.push(Upload {
        bytes,
        file_name: file_name(&media_type).to_owned(),
        media_type,
    });
    Ok(())
}

fn data_url_payload(url: &str) -> Option<&str> {
    url.strip_prefix("data:")?
        .split_once(',')
        .map(|(_, data)| data)
}

fn file_name(media_type: &str) -> &'static str {
    match media_type.split(';').next().unwrap_or(media_type) {
        "image/jpeg" | "image/jpg" => "image.jpg",
        "image/gif" => "image.gif",
        "image/webp" => "image.webp",
        "application/pdf" => "document.pdf",
        _ => "image.png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_web_attachment_and_preserves_tools() {
        let request = json!({
            "model": "ignored",
            "system": [{"type":"text","text":"Be concise"}],
            "messages": [
                {"role":"user","content":"hello"},
                {"role":"assistant","content":[{"type":"text","text":"hi"}]},
                {"role":"user","content":"again"}
            ],
            "max_tokens": 64,
            "tools": [{"name":"lookup","input_schema":{"type":"object"}}]
        });
        let built = build(&request, "claude-sonnet-4-6", "", "UTC").unwrap();
        assert_eq!(built.body["model"], "claude-sonnet-4-6");
        assert_eq!(built.body["tools"][0]["name"], "lookup");
        assert_eq!(built.body["max_tokens_to_sample"], 64);
        assert!(
            built.body["attachments"][0]["extracted_content"]
                .as_str()
                .unwrap()
                .contains("Assistant: hi\n\nHuman: again")
        );
    }

    #[test]
    fn thinking_suffix_selects_extended_mode_without_forwarding_suffix() {
        let request = json!({"messages":[{"role":"user","content":"hello"}]});
        let built = build(&request, "claude-opus-4-6-thinking", "", "UTC").unwrap();
        assert_eq!(built.body["model"], "claude-opus-4-6");
        assert!(built.extended_thinking);
    }
}
