use bytes::Bytes;
use serde_json::{Map, Value, json};

use super::{common, tools};
use crate::TransformError;

pub(super) fn request_to_claude(
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    let input: Value = serde_json::from_slice(&body)?;
    let mut input = common::object(input, "OpenAI Responses")?;
    let messages = responses_input_to_claude(input.remove("input"))?;
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model.into()));
    output.insert("messages".into(), Value::Array(messages));
    output.insert(
        "max_tokens".into(),
        input
            .remove("max_output_tokens")
            .unwrap_or_else(|| Value::from(4096)),
    );
    output.insert("stream".into(), Value::Bool(stream));
    if let Some(instructions) = input.remove("instructions") {
        output.insert("system".into(), instructions);
    }
    copy(&mut input, &mut output, "temperature", "temperature");
    copy(&mut input, &mut output, "top_p", "top_p");
    let parallel = input
        .remove("parallel_tool_calls")
        .and_then(|value| value.as_bool());
    if let Some(tools) = tools::responses_tools_to_claude(input.remove("tools"))? {
        output.insert("tools".into(), tools);
    }
    if let Some(choice) = responses_choice_to_claude(input.remove("tool_choice"), parallel) {
        output.insert("tool_choice".into(), choice);
    }
    if let Some(reasoning) = input.remove("reasoning") {
        output.insert("thinking".into(), reasoning_to_claude(reasoning));
    }
    if let Some(previous) = input.remove("previous_response_id") {
        output.insert(
            "diagnostics".into(),
            json!({"previous_message_id":previous}),
        );
    }
    encode(Value::Object(output))
}

pub(super) fn request_to_responses(
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    let input: Value = serde_json::from_slice(&body)?;
    let mut input = common::object(input, "Claude Messages")?;
    let messages = input
        .remove("messages")
        .and_then(|messages| messages.as_array().cloned())
        .ok_or_else(|| TransformError::shape("Claude Messages", "messages must be an array"))?;
    let mut items = Vec::new();
    for message in messages {
        items.extend(claude_message_to_responses(message)?);
    }
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model.into()));
    output.insert("input".into(), Value::Array(items));
    output.insert("stream".into(), Value::Bool(stream));
    if let Some(system) = input.remove("system") {
        output.insert("instructions".into(), claude_system_to_text(system)?);
    }
    if let Some(max_tokens) = input.remove("max_tokens") {
        output.insert("max_output_tokens".into(), max_tokens);
    }
    copy(&mut input, &mut output, "temperature", "temperature");
    copy(&mut input, &mut output, "top_p", "top_p");
    if let Some(tools) = tools::claude_tools_to_responses(input.remove("tools"))? {
        output.insert("tools".into(), tools);
    }
    if let Some(choice) = tools::claude_choice_to_openai(input.remove("tool_choice")) {
        output.insert("tool_choice".into(), flatten_named_choice(choice));
    }
    if let Some(thinking) = input.remove("thinking") {
        output.insert("reasoning".into(), claude_thinking_to_reasoning(thinking));
    }
    if let Some(previous) = input
        .remove("diagnostics")
        .and_then(|diagnostics| diagnostics.get("previous_message_id").cloned())
    {
        output.insert("previous_response_id".into(), previous);
    }
    encode(Value::Object(output))
}

fn responses_input_to_claude(input: Option<Value>) -> Result<Vec<Value>, TransformError> {
    match input {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(text)) => Ok(vec![json!({"role":"user","content":text})]),
        Some(Value::Array(items)) => items
            .into_iter()
            .map(response_item_to_claude)
            .collect::<Result<Vec<_>, _>>(),
        Some(_) => Err(TransformError::shape(
            "OpenAI Responses",
            "input must be text or an array",
        )),
    }
}

fn response_item_to_claude(item: Value) -> Result<Value, TransformError> {
    let object = item
        .as_object()
        .ok_or_else(|| TransformError::shape("OpenAI Responses input", "item must be an object"))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    match kind {
        "message" => {
            let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
            let role = match role {
                "assistant" => "assistant",
                "system" | "developer" => "system",
                _ => "user",
            };
            let blocks = common::text_blocks(
                object.get("content").cloned().unwrap_or(Value::Null),
                "OpenAI Responses message content",
            )?;
            Ok(json!({"role":role,"content":blocks}))
        }
        "function_call" | "custom_tool_call" => {
            let arguments = object
                .get("arguments")
                .or_else(|| object.get("input"))
                .cloned()
                .unwrap_or_else(|| Value::String("{}".into()));
            let input = match arguments {
                Value::String(arguments) => {
                    serde_json::from_str(&arguments).unwrap_or_else(|_| json!({"value":arguments}))
                }
                value => value,
            };
            Ok(json!({
                "role":"assistant",
                "content":[{
                    "type":"tool_use",
                    "id":object.get("id").or_else(|| object.get("call_id")).cloned().unwrap_or(Value::Null),
                    "name":object.get("name").cloned().unwrap_or(Value::Null),
                    "input":input
                }]
            }))
        }
        "function_call_output" | "custom_tool_call_output" => Ok(json!({
            "role":"user",
            "content":[{
                "type":"tool_result",
                "tool_use_id":object.get("call_id").or_else(|| object.get("id")).cloned().unwrap_or(Value::Null),
                "content":object.get("output").cloned().unwrap_or(Value::String(String::new()))
            }]
        })),
        "reasoning" => {
            let thinking = object
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<String>();
            let signature = object
                .get("encrypted_content")
                .cloned()
                .unwrap_or_else(|| Value::String(String::new()));
            Ok(json!({
                "role":"assistant",
                "content":[{"type":"thinking","thinking":thinking,"signature":signature}]
            }))
        }
        "compaction" => Ok(json!({
            "role":"assistant",
            "content":[{
                "type":"compaction",
                "encrypted_content":object.get("encrypted_content").cloned().unwrap_or(Value::Null)
            }]
        })),
        other => Err(TransformError::unsupported(
            "OpenAI Responses input item",
            other,
        )),
    }
}

fn claude_message_to_responses(message: Value) -> Result<Vec<Value>, TransformError> {
    let object = message
        .as_object()
        .ok_or_else(|| TransformError::shape("Claude Messages", "message must be an object"))?;
    let role = object.get("role").and_then(Value::as_str).unwrap_or("user");
    let blocks = match object.get("content").cloned().unwrap_or(Value::Null) {
        Value::String(text) => vec![json!({"type":"text","text":text})],
        Value::Array(blocks) => blocks,
        _ => {
            return Err(TransformError::shape(
                "Claude Messages",
                "content is invalid",
            ));
        }
    };
    let mut items = Vec::new();
    let mut message_parts = Vec::new();
    for block in blocks {
        let block = block
            .as_object()
            .ok_or_else(|| TransformError::shape("Claude Messages", "block must be an object"))?;
        match block.get("type").and_then(Value::as_str) {
            Some("text") => message_parts.push(json!({
                "type":if role == "assistant" {"output_text"} else {"input_text"},
                "text":block.get("text").cloned().unwrap_or(Value::String(String::new()))
            })),
            Some("image") if role != "assistant" => {
                let part = common::claude_blocks_to_openai(Value::Array(vec![Value::Object(block.clone())]))?
                    .into_iter()
                    .next()
                    .unwrap_or(Value::Null);
                message_parts.push(json!({
                    "type":"input_image",
                    "image_url":part.pointer("/image_url/url").cloned().unwrap_or(Value::Null)
                }));
            }
            Some("tool_use") => items.push(json!({
                "type":"function_call",
                "id":block.get("id").cloned().unwrap_or(Value::Null),
                "call_id":block.get("id").cloned().unwrap_or(Value::Null),
                "name":block.get("name").cloned().unwrap_or(Value::Null),
                "arguments":serde_json::to_string(block.get("input").unwrap_or(&Value::Null))?,
                "status":"completed"
            })),
            Some("tool_result") => items.push(json!({
                "type":"function_call_output",
                "call_id":block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                "output":block.get("content").cloned().unwrap_or(Value::String(String::new()))
            })),
            Some("thinking" | "redacted_thinking") => items.push(json!({
                "type":"reasoning",
                "content":block.get("thinking").cloned().map(|text| json!([{"type":"reasoning_text","text":text}])).unwrap_or(Value::Array(Vec::new())),
                "encrypted_content":block.get("signature").or_else(|| block.get("data")).cloned(),
                "summary":[],
                "status":"completed"
            })),
            Some("compaction") => items.push(json!({
                "type":"compaction",
                "encrypted_content":block.get("encrypted_content").cloned().unwrap_or(Value::Null)
            })),
            Some(other) => return Err(TransformError::unsupported("Claude content block", other)),
            None => return Err(TransformError::shape("Claude content block", "type is missing")),
        }
    }
    if !message_parts.is_empty() {
        items.insert(
            0,
            json!({"type":"message","role":role,"content":message_parts}),
        );
    }
    Ok(items)
}

fn claude_system_to_text(value: Value) -> Result<Value, TransformError> {
    match value {
        Value::String(_) => Ok(value),
        Value::Array(blocks) => {
            let mut text = String::new();
            for block in blocks {
                if block.get("type").and_then(Value::as_str) != Some("text") {
                    return Err(TransformError::unsupported(
                        "Claude system block",
                        block.to_string(),
                    ));
                }
                text.push_str(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
            }
            Ok(Value::String(text))
        }
        _ => Err(TransformError::shape(
            "Claude system",
            "system must be text or blocks",
        )),
    }
}

fn responses_choice_to_claude(value: Option<Value>, parallel: Option<bool>) -> Option<Value> {
    match value {
        Some(Value::String(mode)) => tools::chat_choice_to_claude(Some(Value::String(mode)), parallel),
        Some(Value::Object(choice)) => choice
            .get("name")
            .cloned()
            .map(|name| json!({"type":"tool","name":name,"disable_parallel_tool_use":parallel.map(|value| !value)})),
        _ => None,
    }
}

fn flatten_named_choice(value: Value) -> Value {
    value
        .pointer("/function/name")
        .cloned()
        .map(|name| json!({"type":"function","name":name}))
        .unwrap_or(value)
}

fn reasoning_to_claude(value: Value) -> Value {
    let effort = value.get("effort").cloned();
    json!({"type":"adaptive","effort":effort})
}

fn claude_thinking_to_reasoning(value: Value) -> Value {
    json!({"effort":value.get("effort").cloned().unwrap_or(Value::String("medium".into()))})
}

fn copy(input: &mut Map<String, Value>, output: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = input.remove(from) {
        output.insert(to.into(), value);
    }
}

fn encode(value: Value) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}
