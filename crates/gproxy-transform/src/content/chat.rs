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
    let mut input = common::object(input, "OpenAI Chat")?;
    let messages = input
        .remove("messages")
        .and_then(|messages| messages.as_array().cloned())
        .ok_or_else(|| TransformError::shape("OpenAI Chat", "messages must be an array"))?;
    let mut output_messages = Vec::new();
    let mut system = Vec::new();
    let mut seen_turn = false;
    for message in messages {
        let mut message = message
            .as_object()
            .cloned()
            .ok_or_else(|| TransformError::shape("OpenAI Chat", "message must be an object"))?;
        let role = message
            .remove("role")
            .and_then(|role| role.as_str().map(str::to_owned))
            .ok_or_else(|| TransformError::shape("OpenAI Chat", "message role is missing"))?;
        match role.as_str() {
            "system" | "developer" => {
                let blocks = common::text_blocks(
                    message.remove("content").unwrap_or(Value::Null),
                    "OpenAI Chat system content",
                )?;
                if seen_turn {
                    output_messages.push(json!({"role":"system","content":blocks}));
                } else {
                    system.extend(blocks);
                }
            }
            "user" => {
                seen_turn = true;
                let blocks = common::text_blocks(
                    message.remove("content").unwrap_or(Value::Null),
                    "OpenAI Chat user content",
                )?;
                if !blocks.is_empty() {
                    output_messages.push(json!({"role":"user","content":blocks}));
                }
            }
            "assistant" => {
                seen_turn = true;
                let mut blocks = common::text_blocks(
                    message.remove("content").unwrap_or(Value::Null),
                    "OpenAI Chat assistant content",
                )?;
                if let Some(reasoning) = message
                    .remove("reasoning_content")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .filter(|value| !value.is_empty())
                {
                    blocks.insert(
                        0,
                        json!({"type":"thinking","thinking":reasoning,"signature":""}),
                    );
                }
                if let Some(calls) = message.remove("tool_calls") {
                    blocks.extend(chat_calls_to_claude(calls)?);
                }
                if let Some(call) = message.remove("function_call") {
                    blocks.push(chat_call_to_claude(call, "function_call")?);
                }
                if !blocks.is_empty() {
                    output_messages.push(json!({"role":"assistant","content":blocks}));
                }
            }
            "tool" => {
                seen_turn = true;
                let id = message
                    .remove("tool_call_id")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .ok_or_else(|| {
                        TransformError::shape("OpenAI Chat tool", "tool_call_id is missing")
                    })?;
                let content = message
                    .remove("content")
                    .unwrap_or(Value::String(String::new()));
                output_messages.push(json!({
                    "role":"user",
                    "content":[{"type":"tool_result","tool_use_id":id,"content":content}]
                }));
            }
            "function" => {
                seen_turn = true;
                output_messages.push(json!({
                    "role":"user",
                    "content":message.remove("content").unwrap_or(Value::String(String::new()))
                }));
            }
            other => return Err(TransformError::unsupported("OpenAI Chat role", other)),
        }
    }
    let max_tokens = input
        .remove("max_completion_tokens")
        .or_else(|| input.remove("max_tokens"))
        .unwrap_or_else(|| Value::from(4096));
    let parallel = input
        .remove("parallel_tool_calls")
        .and_then(|value| value.as_bool());
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model.into()));
    output.insert("messages".into(), Value::Array(output_messages));
    output.insert("max_tokens".into(), max_tokens);
    output.insert("stream".into(), Value::Bool(stream));
    if !system.is_empty() {
        output.insert("system".into(), Value::Array(system));
    }
    copy(&mut input, &mut output, "temperature", "temperature");
    copy(&mut input, &mut output, "top_p", "top_p");
    copy(&mut input, &mut output, "stop", "stop_sequences");
    if let Some(tools) = tools::chat_tools_to_claude(input.remove("tools"))? {
        output.insert("tools".into(), tools);
    }
    if let Some(choice) = tools::chat_choice_to_claude(input.remove("tool_choice"), parallel) {
        output.insert("tool_choice".into(), choice);
    }
    encode(Value::Object(output))
}

pub(super) fn request_to_chat(
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    let input: Value = serde_json::from_slice(&body)?;
    let mut input = common::object(input, "Claude Messages")?;
    let mut messages = Vec::new();
    if let Some(system) = input.remove("system") {
        let blocks = common::claude_blocks_to_openai(system)?;
        if !blocks.is_empty() {
            messages.push(json!({"role":"system","content":blocks}));
        }
    }
    let turns = input
        .remove("messages")
        .and_then(|messages| messages.as_array().cloned())
        .ok_or_else(|| TransformError::shape("Claude Messages", "messages must be an array"))?;
    for turn in turns {
        let turn = turn
            .as_object()
            .ok_or_else(|| TransformError::shape("Claude Messages", "message must be an object"))?;
        let role = turn.get("role").and_then(Value::as_str).unwrap_or("user");
        match role {
            "assistant" => messages.push(claude_assistant_to_chat(turn)?),
            "user" => messages.extend(claude_user_to_chat(turn)?),
            "system" => {
                let blocks = common::claude_blocks_to_openai(
                    turn.get("content").cloned().unwrap_or(Value::Null),
                )?;
                messages.push(json!({"role":"developer","content":blocks}));
            }
            other => return Err(TransformError::unsupported("Claude message role", other)),
        }
    }
    let mut output = Map::new();
    output.insert("model".into(), Value::String(model.into()));
    output.insert("messages".into(), Value::Array(messages));
    output.insert("stream".into(), Value::Bool(stream));
    if let Some(max_tokens) = input.remove("max_tokens") {
        output.insert("max_completion_tokens".into(), max_tokens);
    }
    copy(&mut input, &mut output, "temperature", "temperature");
    copy(&mut input, &mut output, "top_p", "top_p");
    copy(&mut input, &mut output, "stop_sequences", "stop");
    if let Some(tools) = tools::claude_tools_to_chat(input.remove("tools"))? {
        output.insert("tools".into(), tools);
    }
    if let Some(choice) = tools::claude_choice_to_openai(input.remove("tool_choice")) {
        output.insert("tool_choice".into(), choice);
    }
    encode(Value::Object(output))
}

fn chat_calls_to_claude(value: Value) -> Result<Vec<Value>, TransformError> {
    value
        .as_array()
        .ok_or_else(|| TransformError::shape("OpenAI Chat", "tool_calls must be an array"))?
        .iter()
        .cloned()
        .map(|call| chat_call_to_claude(call, "tool_call"))
        .collect()
}

fn chat_call_to_claude(value: Value, fallback_id: &str) -> Result<Value, TransformError> {
    let mut call = value
        .as_object()
        .cloned()
        .ok_or_else(|| TransformError::shape("OpenAI Chat tool call", "call must be an object"))?;
    let id = call
        .remove("id")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| fallback_id.into());
    let mut function = call
        .remove("function")
        .unwrap_or(Value::Object(call))
        .as_object()
        .cloned()
        .ok_or_else(|| TransformError::shape("OpenAI Chat tool call", "function is missing"))?;
    let name = function
        .remove("name")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| TransformError::shape("OpenAI Chat tool call", "name is missing"))?;
    let arguments = function
        .remove("arguments")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "{}".into());
    let input =
        serde_json::from_str::<Value>(&arguments).unwrap_or_else(|_| json!({"value":arguments}));
    Ok(json!({"type":"tool_use","id":id,"name":name,"input":input}))
}

fn claude_assistant_to_chat(turn: &Map<String, Value>) -> Result<Value, TransformError> {
    let content = turn.get("content").cloned().unwrap_or(Value::Null);
    let blocks = match content {
        Value::String(text) => vec![json!({"type":"text","text":text})],
        Value::Array(blocks) => blocks,
        _ => {
            return Err(TransformError::shape(
                "Claude assistant",
                "content is invalid",
            ));
        }
    };
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut calls = Vec::new();
    for block in blocks {
        let block = block
            .as_object()
            .ok_or_else(|| TransformError::shape("Claude assistant", "block must be an object"))?;
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text.push(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ),
            Some("thinking") => reasoning.push(
                block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ),
            Some("redacted_thinking") => {}
            Some("tool_use") => calls.push(json!({
                "id":block.get("id").cloned().unwrap_or(Value::Null),
                "type":"function",
                "function":{
                    "name":block.get("name").cloned().unwrap_or(Value::Null),
                    "arguments":serde_json::to_string(block.get("input").unwrap_or(&Value::Null))?
                }
            })),
            Some(other) => {
                return Err(TransformError::unsupported("Claude assistant block", other));
            }
            None => {
                return Err(TransformError::shape(
                    "Claude assistant",
                    "block type is missing",
                ));
            }
        }
    }
    let mut message = json!({"role":"assistant","content":text.join("")});
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning.join(""));
    }
    if !calls.is_empty() {
        message["tool_calls"] = Value::Array(calls);
    }
    Ok(message)
}

fn claude_user_to_chat(turn: &Map<String, Value>) -> Result<Vec<Value>, TransformError> {
    let content = turn.get("content").cloned().unwrap_or(Value::Null);
    let blocks = match content {
        Value::String(text) => return Ok(vec![json!({"role":"user","content":text})]),
        Value::Array(blocks) => blocks,
        _ => return Err(TransformError::shape("Claude user", "content is invalid")),
    };
    let mut messages = Vec::new();
    let mut parts = Vec::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) == Some("tool_result") {
            if !parts.is_empty() {
                messages.push(json!({"role":"user","content":std::mem::take(&mut parts)}));
            }
            messages.push(json!({
                "role":"tool",
                "tool_call_id":block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                "content":block.get("content").cloned().unwrap_or(Value::String(String::new()))
            }));
        } else {
            parts.extend(common::claude_blocks_to_openai(Value::Array(vec![block]))?);
        }
    }
    if !parts.is_empty() {
        messages.push(json!({"role":"user","content":parts}));
    }
    Ok(messages)
}

fn copy(input: &mut Map<String, Value>, output: &mut Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = input.remove(from) {
        output.insert(to.into(), value);
    }
}

fn encode(value: Value) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}
