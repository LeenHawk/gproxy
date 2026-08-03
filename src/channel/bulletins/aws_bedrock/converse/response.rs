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
    let usage = root.remove("usage").map(usage).unwrap_or_else(|| json!({}));
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
        return json!({});
    };
    let cache_creation = cache_creation(&mut usage);
    let (Some(input_tokens), Some(output_tokens)) = (
        usage.remove("inputTokens").and_then(|value| value.as_u64()),
        usage
            .remove("outputTokens")
            .and_then(|value| value.as_u64()),
    ) else {
        return json!({});
    };
    let mut mapped = json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "cache_read_input_tokens": usage.remove("cacheReadInputTokens").unwrap_or(Value::from(0)),
        "cache_creation_input_tokens": usage.remove("cacheWriteInputTokens").unwrap_or(Value::from(0))
    });
    if let Some(cache_creation) = cache_creation {
        mapped
            .as_object_mut()
            .expect("mapped usage is an object")
            .insert("cache_creation".into(), cache_creation);
    }
    mapped
}

fn cache_creation(usage: &mut Map<String, Value>) -> Option<Value> {
    let details = usage.remove("cacheDetails")?;
    let details = details.as_array()?;
    let mut five_minutes = 0u64;
    let mut one_hour = 0u64;
    let mut recognized = false;
    for detail in details {
        let Some(tokens) = detail.get("inputTokens").and_then(Value::as_u64) else {
            continue;
        };
        match detail.get("ttl").and_then(Value::as_str) {
            Some("5m") => {
                five_minutes = five_minutes.saturating_add(tokens);
                recognized = true;
            }
            Some("1h") => {
                one_hour = one_hour.saturating_add(tokens);
                recognized = true;
            }
            _ => {}
        }
    }
    recognized.then(|| {
        json!({
            "ephemeral_5m_input_tokens": five_minutes,
            "ephemeral_1h_input_tokens": one_hour
        })
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
