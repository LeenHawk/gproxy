use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use crate::channel::ChannelError;
use crate::protocol::openai::{ChatCompletionRequest, ChatTool, FunctionDefinition};

pub struct Upload {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub file_name: String,
}

pub struct TaskletRequest {
    pub message: String,
    pub uploads: Vec<Upload>,
    pub file_ids: Vec<String>,
    pub model: String,
    pub tools: Vec<ChatTool>,
    pub tool_choice: Option<Value>,
}

pub fn parse(body: &[u8], upstream_model: &str) -> Result<TaskletRequest, ChannelError> {
    let typed: ChatCompletionRequest = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Build(format!("tasklet chat request: {error}")))?;
    let raw: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Build(format!("tasklet chat request: {error}")))?;
    let messages = raw
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| ChannelError::Build("tasklet request has no messages".into()))?;
    let mut uploads = Vec::new();
    let mut file_ids = Vec::new();
    let mut rendered = Vec::new();
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let mut text = content_text(message.get("content"), &mut uploads, &mut file_ids)?;
        if let Some(calls) = message.get("tool_calls").and_then(Value::as_array)
            && !calls.is_empty()
        {
            let calls = serde_json::to_string(calls)
                .map_err(|error| ChannelError::Build(format!("tasklet tool history: {error}")))?;
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str("Requested client tool calls: ");
            text.push_str(&calls);
        }
        let role = message
            .get("tool_call_id")
            .and_then(Value::as_str)
            .map(|id| format!("tool result for {id}"))
            .unwrap_or_else(|| role.to_owned());
        if !text.trim().is_empty() {
            rendered.push((role, text));
        }
    }
    let message = if rendered.len() == 1 && rendered[0].0 == "user" {
        rendered.remove(0).1
    } else {
        rendered
            .into_iter()
            .map(|(role, text)| format!("{role}: {text}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    if message.trim().is_empty() && uploads.is_empty() && file_ids.is_empty() {
        return Err(ChannelError::Build(
            "tasklet request has no usable content".into(),
        ));
    }
    let model = if upstream_model.trim().is_empty() {
        serde_json::to_value(typed.model)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "tasklet-standard".into())
    } else {
        upstream_model.to_owned()
    };
    let mut tools = typed.tools.unwrap_or_default();
    tools.extend(
        typed
            .functions
            .unwrap_or_default()
            .into_iter()
            .map(|function| ChatTool::Function {
                function: FunctionDefinition {
                    name: function.name,
                    description: function.description,
                    parameters: function.parameters,
                    strict: None,
                    extra: function.extra,
                },
                extra: Default::default(),
            }),
    );
    let tool_choice = raw
        .get("tool_choice")
        .or_else(|| raw.get("function_call"))
        .cloned();
    if tool_choice.as_ref().and_then(Value::as_str) == Some("none") {
        tools.clear();
    }
    Ok(TaskletRequest {
        message,
        uploads,
        file_ids,
        model,
        tools,
        tool_choice,
    })
}

pub fn attach_tool_bridge(request: &mut TaskletRequest, turn_id: &str) -> Result<(), ChannelError> {
    let tools = serde_json::to_string(&request.tools)
        .map_err(|error| ChannelError::Build(format!("tasklet tool catalogue: {error}")))?;
    let choice = request
        .tool_choice
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| ChannelError::Build(format!("tasklet tool choice: {error}")))?
        .unwrap_or_else(|| "\"auto\"".into());
    request.message.push_str(&format!(
        "\n\n<gproxy_client_tool_bridge>\n\
         Client-side tools are available but must not be executed inside Tasklet. \
         When one is needed, call the connected MCP tool `gproxy_call_client_tool` exactly once \
         with turn_id `{turn_id}`, the exact tool name, and arguments matching its schema. \
         For a custom text tool, pass its text as an `input` field inside arguments. \
         After that MCP call, stop this turn without describing or simulating the result.\n\
         tool_choice: {choice}\nclient_tools: {tools}\n\
         </gproxy_client_tool_bridge>"
    ));
    Ok(())
}

pub fn send_body(
    request: &TaskletRequest,
    uploaded: Vec<String>,
    workspace_id: &str,
    timezone: &str,
) -> Value {
    let mut file_ids = request.file_ids.clone();
    file_ids.extend(uploaded);
    let mut body = json!({
        "agentId": "new",
        "message": request.message,
        "timezone": timezone,
        "fileIds": file_ids,
        "workspaceId": workspace_id,
        "agentConfig": {"preview": true},
    });
    if let Some(intelligence) = intelligence(&request.model) {
        body["intelligence"] = Value::String(intelligence.into());
    } else {
        body["modelConfig"] = model_config(&request.model);
    }
    body
}

fn content_text(
    content: Option<&Value>,
    uploads: &mut Vec<Upload>,
    file_ids: &mut Vec<String>,
) -> Result<String, ChannelError> {
    let Some(content) = content else {
        return Ok(String::new());
    };
    if let Some(text) = content.as_str() {
        return Ok(text.to_owned());
    }
    let mut parts = Vec::new();
    for part in content.as_array().into_iter().flatten() {
        match part.get("type").and_then(Value::as_str) {
            Some("text") => push_text(&mut parts, part.get("text")),
            Some("image_url") => {
                let url = part.pointer("/image_url/url").and_then(Value::as_str);
                collect_data(url, None, "image.png", uploads, &mut parts)?;
            }
            Some("input_audio") => {
                let data = part.pointer("/input_audio/data").and_then(Value::as_str);
                let format = part
                    .pointer("/input_audio/format")
                    .and_then(Value::as_str)
                    .unwrap_or("wav");
                collect_data(
                    data,
                    Some(&format!("audio/{format}")),
                    &format!("audio.{format}"),
                    uploads,
                    &mut parts,
                )?;
            }
            Some("file") => collect_file(part.get("file"), uploads, file_ids)?,
            _ => {}
        }
    }
    Ok(parts.join("\n"))
}

fn push_text(parts: &mut Vec<String>, value: Option<&Value>) {
    if let Some(text) = value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        parts.push(text.to_owned());
    }
}

fn collect_file(
    file: Option<&Value>,
    uploads: &mut Vec<Upload>,
    file_ids: &mut Vec<String>,
) -> Result<(), ChannelError> {
    let Some(file) = file else { return Ok(()) };
    if let Some(id) = file.get("file_id").and_then(Value::as_str) {
        if id.starts_with("f_") {
            file_ids.push(id.to_owned());
            return Ok(());
        }
        return Err(ChannelError::Build(
            "tasklet only accepts Tasklet f_ file ids".into(),
        ));
    }
    let name = file
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or("attachment.bin");
    collect_data(
        file.get("file_data").and_then(Value::as_str),
        Some("application/octet-stream"),
        name,
        uploads,
        &mut Vec::new(),
    )
}

fn collect_data(
    data: Option<&str>,
    fallback_media_type: Option<&str>,
    file_name: &str,
    uploads: &mut Vec<Upload>,
    text_parts: &mut Vec<String>,
) -> Result<(), ChannelError> {
    let Some(data) = data else { return Ok(()) };
    if !data.starts_with("data:") && data.contains("://") {
        text_parts.push(format!("Attachment URL: {data}"));
        return Ok(());
    }
    let (media_type, payload) = if let Some(rest) = data.strip_prefix("data:") {
        let (metadata, payload) = rest
            .split_once(',')
            .ok_or_else(|| ChannelError::Build("bad tasklet data URL".into()))?;
        (
            metadata
                .split(';')
                .next()
                .unwrap_or("application/octet-stream"),
            payload,
        )
    } else {
        (
            fallback_media_type.unwrap_or("application/octet-stream"),
            data,
        )
    };
    let bytes = STANDARD
        .decode(payload)
        .map_err(|error| ChannelError::Build(format!("tasklet attachment base64: {error}")))?;
    uploads.push(Upload {
        bytes,
        media_type: media_type.to_owned(),
        file_name: file_name.to_owned(),
    });
    Ok(())
}

fn intelligence(model: &str) -> Option<&'static str> {
    match model {
        "tasklet-standard" | "tasklet-basic" => Some("standard"),
        "tasklet-advanced" => Some("advanced"),
        "tasklet-expert" => Some("expert"),
        "tasklet-genius" => Some("genius"),
        _ => None,
    }
}

fn model_config(model: &str) -> Value {
    let effort = if model.contains("luna") || model.contains("haiku") || model.contains("flash") {
        "low"
    } else if model.contains("opus") || model.contains("fable") || model.contains("pro_") {
        "high"
    } else {
        "medium"
    };
    json!({
        "model": model,
        "thinkingEffort": effort,
        "chatHistory": if model.contains("fable") { "extended" } else { "default" },
        "preset": null,
    })
}
