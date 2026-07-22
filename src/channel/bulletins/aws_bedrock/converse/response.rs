use bytes::Bytes;
use serde_json::{Map, Value, json};

pub(super) fn convert(body: Bytes) -> Bytes {
    let Ok(Value::Object(mut root)) = serde_json::from_slice(body.as_ref()) else {
        return body;
    };
    let Some(Value::Object(mut output)) = root.remove("output") else {
        return body;
    };
    let Some(Value::Object(mut message)) = output.remove("message") else {
        return body;
    };
    let content = message
        .remove("content")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(content_block)
        .collect::<Vec<_>>();
    let usage = root.remove("usage").map(usage).unwrap_or_else(|| {
        json!({
            "input_tokens": 0, "output_tokens": 0
        })
    });
    Bytes::from(
        json!({
            "id": format!("msg_{}", crate::util::id::ulid().to_ascii_lowercase()),
            "type": "message",
            "role": "assistant",
            "model": "aws-bedrock",
            "content": content,
            "stop_reason": stop_reason(root.remove("stopReason")),
            "stop_sequence": Value::Null,
            "usage": usage
        })
        .to_string(),
    )
}

fn content_block(value: Value) -> Option<Value> {
    let Value::Object(mut block) = value else {
        return None;
    };
    if let Some(text) = block.remove("text") {
        return Some(json!({ "type": "text", "text": text }));
    }
    if let Some(Value::Object(mut tool)) = block.remove("toolUse") {
        return Some(json!({
            "type": "tool_use",
            "id": tool.remove("toolUseId").unwrap_or(Value::String("tool".into())),
            "name": tool.remove("name").unwrap_or(Value::String("tool".into())),
            "input": tool.remove("input").unwrap_or_else(|| json!({}))
        }));
    }
    if let Some(Value::Object(mut reasoning)) = block.remove("reasoningContent")
        && let Some(Value::Object(mut text)) = reasoning.remove("reasoningText")
    {
        let mut result = Map::new();
        result.insert("type".into(), Value::String("thinking".into()));
        result.insert(
            "thinking".into(),
            text.remove("text").unwrap_or(Value::String(String::new())),
        );
        if let Some(signature) = text.remove("signature") {
            result.insert("signature".into(), signature);
        }
        return Some(Value::Object(result));
    }
    None
}

pub(super) fn usage(value: Value) -> Value {
    let Value::Object(mut usage) = value else {
        return json!({"input_tokens":0,"output_tokens":0});
    };
    json!({
        "input_tokens": usage.remove("inputTokens").unwrap_or(Value::from(0)),
        "output_tokens": usage.remove("outputTokens").unwrap_or(Value::from(0)),
        "cache_read_input_tokens": usage.remove("cacheReadInputTokens").unwrap_or(Value::from(0)),
        "cache_creation_input_tokens": usage.remove("cacheWriteInputTokens").unwrap_or(Value::from(0))
    })
}

pub(super) fn stop_reason(value: Option<Value>) -> Value {
    let reason = value.as_ref().and_then(Value::as_str).unwrap_or("end_turn");
    Value::String(
        match reason {
            "tool_use" => "tool_use",
            "max_tokens" => "max_tokens",
            "stop_sequence" => "stop_sequence",
            "guardrail_intervened" | "content_filtered" => "refusal",
            _ => "end_turn",
        }
        .into(),
    )
}
