use serde_json::{Map, Value, json};

use crate::TransformError;

pub(super) fn object(
    value: Value,
    wire: &'static str,
) -> Result<Map<String, Value>, TransformError> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| TransformError::shape(wire, "root must be an object"))
}

pub(super) fn text_blocks(value: Value, wire: &'static str) -> Result<Vec<Value>, TransformError> {
    match value {
        Value::String(text) => Ok(nonempty(text)
            .into_iter()
            .map(|text| json!({"type":"text","text":text}))
            .collect()),
        Value::Array(parts) => parts
            .into_iter()
            .filter_map(|part| match part {
                Value::Object(mut part) => match part.get("type").and_then(Value::as_str) {
                    Some("text" | "input_text" | "output_text") => part
                        .remove("text")
                        .and_then(|text| text.as_str().map(str::to_owned))
                        .and_then(nonempty)
                        .map(|text| Ok(json!({"type":"text","text":text}))),
                    Some("image_url" | "input_image") => {
                        let url = part.remove("image_url").and_then(|value| match value {
                            Value::String(url) => Some(url),
                            Value::Object(mut image) => image
                                .remove("url")
                                .and_then(|url| url.as_str().map(str::to_owned)),
                            _ => None,
                        });
                        url.map(|url| Ok(image_to_claude(url)))
                    }
                    Some(other) => Some(Err(TransformError::unsupported(wire, other))),
                    None => Some(Err(TransformError::shape(
                        wire,
                        "content part type is missing",
                    ))),
                },
                _ => Some(Err(TransformError::shape(
                    wire,
                    "content part must be an object",
                ))),
            })
            .collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err(TransformError::shape(
            wire,
            "content must be text or an array",
        )),
    }
}

pub(super) fn claude_blocks_to_openai(value: Value) -> Result<Vec<Value>, TransformError> {
    match value {
        Value::String(text) => Ok(vec![json!({"type":"text","text":text})]),
        Value::Array(blocks) => blocks
            .into_iter()
            .map(|block| {
                let object = block.as_object().ok_or_else(|| {
                    TransformError::shape("Claude messages", "content block must be an object")
                })?;
                match object.get("type").and_then(Value::as_str) {
                    Some("text") => Ok(json!({
                        "type":"text",
                        "text": object.get("text").cloned().unwrap_or(Value::String(String::new()))
                    })),
                    Some("image") => claude_image_to_openai(object),
                    Some(other) => Err(TransformError::unsupported("Claude messages", other)),
                    None => Err(TransformError::shape(
                        "Claude messages",
                        "content block type is missing",
                    )),
                }
            })
            .collect(),
        _ => Err(TransformError::shape(
            "Claude messages",
            "content must be text or an array",
        )),
    }
}

pub(crate) fn usage_to_claude(usage: Option<&Value>, chat: bool) -> Value {
    let usage = usage.and_then(Value::as_object);
    let (input_name, output_name, input_details, output_details) = if chat {
        (
            "prompt_tokens",
            "completion_tokens",
            "prompt_tokens_details",
            "completion_tokens_details",
        )
    } else {
        (
            "input_tokens",
            "output_tokens",
            "input_tokens_details",
            "output_tokens_details",
        )
    };
    let input = field(usage, input_name);
    let output = field(usage, output_name);
    let cached = nested_field(usage, input_details, "cached_tokens").min(input);
    let cache_write =
        nested_field(usage, input_details, "cache_write_tokens").min(input.saturating_sub(cached));
    let mut value = json!({
        "input_tokens": input.saturating_sub(cached).saturating_sub(cache_write),
        "output_tokens": output,
        "cache_read_input_tokens": cached,
        "cache_creation_input_tokens": cache_write,
    });
    if let Some(reasoning) = usage
        .and_then(|usage| usage.get(output_details))
        .and_then(|details| details.get("reasoning_tokens"))
        .cloned()
    {
        value["reasoning_tokens"] = reasoning;
    }
    value
}

pub(crate) fn usage_to_openai(usage: Option<&Value>, chat: bool) -> Value {
    let usage = usage.and_then(Value::as_object);
    let uncached = field(usage, "input_tokens");
    let cached = field(usage, "cache_read_input_tokens");
    let cache_write = field(usage, "cache_creation_input_tokens");
    let input = uncached.saturating_add(cached).saturating_add(cache_write);
    let output = field(usage, "output_tokens");
    if chat {
        json!({
            "prompt_tokens":input,
            "completion_tokens":output,
            "total_tokens":input.saturating_add(output),
            "prompt_tokens_details":{
                "cached_tokens":cached,
                "cache_write_tokens":cache_write,
            }
        })
    } else {
        json!({
            "input_tokens":input,
            "output_tokens":output,
            "total_tokens":input.saturating_add(output),
            "input_tokens_details":{
                "cached_tokens":cached,
                "cache_write_tokens":cache_write,
            }
        })
    }
}

pub(crate) fn stop_to_claude(reason: Option<&str>) -> &'static str {
    match reason {
        Some("length") => "max_tokens",
        Some("tool_calls" | "function_call") => "tool_use",
        Some("content_filter") => "refusal",
        _ => "end_turn",
    }
}

pub(crate) fn stop_to_openai(reason: Option<&str>) -> &'static str {
    match reason {
        Some("max_tokens" | "model_context_window_exceeded") => "length",
        Some("tool_use") => "tool_calls",
        Some("refusal") => "content_filter",
        _ => "stop",
    }
}

fn image_to_claude(url: String) -> Value {
    if let Some(data) = url.strip_prefix("data:")
        && let Some((media_type, payload)) = data.split_once(";base64,")
    {
        return json!({
            "type":"image",
            "source":{"type":"base64","media_type":media_type,"data":payload}
        });
    }
    json!({"type":"image","source":{"type":"url","url":url}})
}

fn claude_image_to_openai(object: &Map<String, Value>) -> Result<Value, TransformError> {
    let source = object
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| TransformError::shape("Claude image", "source is missing"))?;
    let url = match source.get("type").and_then(Value::as_str) {
        Some("url") => source.get("url").and_then(Value::as_str).map(str::to_owned),
        Some("base64") => Some(format!(
            "data:{};base64,{}",
            source
                .get("media_type")
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream"),
            source
                .get("data")
                .and_then(Value::as_str)
                .unwrap_or_default()
        )),
        Some(other) => return Err(TransformError::unsupported("Claude image source", other)),
        None => None,
    }
    .ok_or_else(|| TransformError::shape("Claude image", "source data is missing"))?;
    Ok(json!({"type":"image_url","image_url":{"url":url}}))
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn field(object: Option<&Map<String, Value>>, name: &str) -> u64 {
    object
        .and_then(|object| object.get(name))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn nested_field(object: Option<&Map<String, Value>>, parent: &str, name: &str) -> u64 {
    object
        .and_then(|object| object.get(parent))
        .and_then(|parent| parent.get(name))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}
