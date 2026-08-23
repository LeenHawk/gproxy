use bytes::Bytes;
use serde_json::{Value, json};

use super::common;
use crate::TransformError;

pub(crate) fn claude_to_chat_response(body: Bytes) -> Result<Bytes, TransformError> {
    let input: Value = serde_json::from_slice(&body)?;
    let input = input
        .as_object()
        .ok_or_else(|| TransformError::shape("Claude response", "root must be an object"))?;
    let (content, reasoning, calls) = claude_output(input.get("content"))?;
    let mut message = json!({"role":"assistant","content":content});
    if !reasoning.is_empty() {
        message["reasoning_content"] = Value::String(reasoning);
    }
    if !calls.is_empty() {
        message["tool_calls"] = Value::Array(calls);
    }
    encode(json!({
        "id":input.get("id").cloned().unwrap_or(Value::String("chatcmpl_gproxy".into())),
        "object":"chat.completion",
        "created":0,
        "model":input.get("model").cloned().unwrap_or(Value::String("unknown".into())),
        "choices":[{"index":0,"message":message,"finish_reason":common::stop_to_openai(input.get("stop_reason").and_then(Value::as_str))}],
        "usage":common::usage_to_openai(input.get("usage"), true)
    }))
}

pub(crate) fn chat_to_claude_response(body: Bytes) -> Result<Bytes, TransformError> {
    let input: Value = serde_json::from_slice(&body)?;
    let input = input
        .as_object()
        .ok_or_else(|| TransformError::shape("OpenAI Chat response", "root must be an object"))?;
    let choice = input
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(Value::as_object)
        .ok_or_else(|| TransformError::shape("OpenAI Chat response", "choice is missing"))?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| TransformError::shape("OpenAI Chat response", "message is missing"))?;
    let mut content = Vec::new();
    if let Some(reasoning) = message
        .get("reasoning_content")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        content.push(json!({"type":"thinking","thinking":reasoning,"signature":""}));
    }
    if let Some(text) = message
        .get("content")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        content.push(json!({"type":"text","text":text}));
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            content.push(chat_call_to_claude(call)?);
        }
    }
    if content.is_empty() {
        content.push(json!({"type":"text","text":""}));
    }
    encode(json!({
        "id":input.get("id").cloned().unwrap_or(Value::String("msg_gproxy".into())),
        "type":"message",
        "role":"assistant",
        "model":input.get("model").cloned().unwrap_or(Value::String("unknown".into())),
        "content":content,
        "stop_reason":common::stop_to_claude(choice.get("finish_reason").and_then(Value::as_str)),
        "stop_sequence":null,
        "usage":common::usage_to_claude(input.get("usage"), true)
    }))
}

pub(crate) fn claude_to_responses_response(body: Bytes) -> Result<Bytes, TransformError> {
    let input: Value = serde_json::from_slice(&body)?;
    let input = input
        .as_object()
        .ok_or_else(|| TransformError::shape("Claude response", "root must be an object"))?;
    let id = input
        .get("id")
        .cloned()
        .unwrap_or(Value::String("resp_gproxy".into()));
    let (output, output_text) = claude_output_to_responses(input.get("content"), &id)?;
    let incomplete = matches!(
        input.get("stop_reason").and_then(Value::as_str),
        Some("max_tokens" | "model_context_window_exceeded" | "refusal")
    );
    encode(json!({
        "id":id,
        "object":"response",
        "created_at":0,
        "completed_at":if incomplete {Value::Null} else {Value::from(0)},
        "status":if incomplete {"incomplete"} else {"completed"},
        "incomplete_details":if incomplete {json!({"reason":common::stop_to_openai(input.get("stop_reason").and_then(Value::as_str))})} else {Value::Null},
        "model":input.get("model").cloned().unwrap_or(Value::String("unknown".into())),
        "output":output,
        "output_text":output_text,
        "usage":common::usage_to_openai(input.get("usage"), false)
    }))
}

pub(crate) fn responses_to_claude_response(body: Bytes) -> Result<Bytes, TransformError> {
    let input: Value = serde_json::from_slice(&body)?;
    let input = input
        .as_object()
        .ok_or_else(|| TransformError::shape("OpenAI Responses", "root must be an object"))?;
    let mut content = Vec::new();
    for item in input
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        content.extend(response_item_to_claude(item)?);
    }
    let stop = match input.get("status").and_then(Value::as_str) {
        Some("incomplete") => "max_tokens",
        Some("failed") => "refusal",
        _ if content
            .iter()
            .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_use")) =>
        {
            "tool_use"
        }
        _ => "end_turn",
    };
    encode(json!({
        "id":input.get("id").cloned().unwrap_or(Value::String("msg_gproxy".into())),
        "type":"message",
        "role":"assistant",
        "model":input.get("model").cloned().unwrap_or(Value::String("unknown".into())),
        "content":content,
        "stop_reason":stop,
        "stop_sequence":null,
        "usage":common::usage_to_claude(input.get("usage"), false)
    }))
}

fn claude_output(content: Option<&Value>) -> Result<(String, String, Vec<Value>), TransformError> {
    let blocks = content
        .and_then(Value::as_array)
        .ok_or_else(|| TransformError::shape("Claude response", "content must be an array"))?;
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut calls = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => text.push_str(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            Some("thinking") => reasoning.push_str(
                block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
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
            Some(other) => return Err(TransformError::unsupported("Claude response block", other)),
            None => {
                return Err(TransformError::shape(
                    "Claude response",
                    "block type is missing",
                ));
            }
        }
    }
    Ok((text, reasoning, calls))
}

fn claude_output_to_responses(
    content: Option<&Value>,
    message_id: &Value,
) -> Result<(Vec<Value>, String), TransformError> {
    let blocks = content
        .and_then(Value::as_array)
        .ok_or_else(|| TransformError::shape("Claude response", "content must be an array"))?;
    let mut output = Vec::new();
    let mut text_parts = Vec::new();
    let mut message_parts = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block.get("text").cloned().unwrap_or(Value::String(String::new()));
                text_parts.push(text.as_str().unwrap_or_default().to_owned());
                message_parts.push(json!({"type":"output_text","text":text,"annotations":[]}));
            }
            Some("thinking" | "redacted_thinking") => output.push(json!({
                "type":"reasoning",
                "content":block.get("thinking").cloned().map(|text| json!([{"type":"reasoning_text","text":text}])).unwrap_or(Value::Array(Vec::new())),
                "encrypted_content":block.get("signature").or_else(|| block.get("data")).cloned(),
                "summary":[],"status":"completed"
            })),
            Some("tool_use") => output.push(json!({
                "type":"function_call",
                "id":block.get("id").cloned().unwrap_or(Value::Null),
                "call_id":block.get("id").cloned().unwrap_or(Value::Null),
                "name":block.get("name").cloned().unwrap_or(Value::Null),
                "arguments":serde_json::to_string(block.get("input").unwrap_or(&Value::Null))?,
                "status":"completed"
            })),
            Some(other) => return Err(TransformError::unsupported("Claude response block", other)),
            None => return Err(TransformError::shape("Claude response", "block type is missing")),
        }
    }
    if !message_parts.is_empty() {
        output.push(json!({
            "type":"message","id":message_id,"role":"assistant",
            "content":message_parts,"status":"completed"
        }));
    }
    Ok((output, text_parts.join("")))
}

fn response_item_to_claude(item: &Value) -> Result<Vec<Value>, TransformError> {
    match item.get("type").and_then(Value::as_str) {
        Some("message") => item
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|part| match part.get("type").and_then(Value::as_str) {
                Some("output_text") => Ok(json!({"type":"text","text":part.get("text").cloned().unwrap_or(Value::String(String::new()))})),
                Some("refusal") => Ok(json!({"type":"text","text":part.get("refusal").cloned().unwrap_or(Value::String(String::new()))})),
                Some(other) => Err(TransformError::unsupported("OpenAI response message part", other)),
                None => Err(TransformError::shape("OpenAI response message", "part type is missing")),
            })
            .collect(),
        Some("function_call" | "custom_tool_call") => {
            let arguments = item.get("arguments").or_else(|| item.get("input")).and_then(Value::as_str).unwrap_or("{}");
            let input = serde_json::from_str(arguments).unwrap_or_else(|_| json!({"value":arguments}));
            Ok(vec![json!({
                "type":"tool_use",
                "id":item.get("id").or_else(|| item.get("call_id")).cloned().unwrap_or(Value::Null),
                "name":item.get("name").cloned().unwrap_or(Value::Null),
                "input":input
            })])
        }
        Some("reasoning") => {
            let thinking = item.get("content").and_then(Value::as_array).into_iter().flatten()
                .filter_map(|part| part.get("text").and_then(Value::as_str)).collect::<String>();
            Ok(vec![json!({
                "type":"thinking","thinking":thinking,
                "signature":item.get("encrypted_content").cloned().unwrap_or(Value::String(String::new()))
            })])
        }
        Some(other) => Err(TransformError::unsupported("OpenAI response item", other)),
        None => Err(TransformError::shape("OpenAI response item", "type is missing")),
    }
}

fn chat_call_to_claude(call: &Value) -> Result<Value, TransformError> {
    let function = call
        .get("function")
        .ok_or_else(|| TransformError::shape("OpenAI Chat response", "tool function is missing"))?;
    let arguments = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    Ok(json!({
        "type":"tool_use",
        "id":call.get("id").cloned().unwrap_or(Value::String("tool_gproxy".into())),
        "name":function.get("name").cloned().unwrap_or(Value::Null),
        "input":serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({"value":arguments}))
    }))
}

fn encode(value: Value) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}
