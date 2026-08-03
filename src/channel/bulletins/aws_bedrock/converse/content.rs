use serde_json::{Map, Value, json};

pub(super) fn content(value: Value, assistant: bool) -> Vec<Value> {
    match value {
        Value::String(text) => vec![json!({ "text": text })],
        Value::Array(blocks) => blocks
            .into_iter()
            .flat_map(|block| content_block(block, assistant))
            .collect(),
        Value::Object(block) => content_block(Value::Object(block), assistant),
        _ => Vec::new(),
    }
}

fn content_block(value: Value, assistant: bool) -> Vec<Value> {
    let Value::Object(mut block) = value else {
        return Vec::new();
    };
    let cache_point = block.remove("cache_control").map(super::cache_point);
    let kind = block.get("type").and_then(Value::as_str).unwrap_or("text");
    let mapped = match kind {
        "text" => block.remove("text").map(|text| json!({ "text": text })),
        "tool_use" => Some(json!({ "toolUse": {
            "toolUseId": block.remove("id").unwrap_or(Value::String("tool".into())),
            "name": block.remove("name").unwrap_or(Value::String("tool".into())),
            "input": block.remove("input").unwrap_or_else(|| json!({}))
        }})),
        "tool_result" => Some(tool_result(block)),
        "thinking" if assistant => Some(reasoning(block)),
        "image" => media(block, "image"),
        "document" => media(block, "document"),
        _ => None,
    };
    let mut output: Vec<_> = mapped.into_iter().collect();
    if let Some(cache_point) = cache_point
        && !output.is_empty()
    {
        output.push(cache_point);
    }
    output
}

fn tool_result(mut block: Map<String, Value>) -> Value {
    let content = block
        .remove("content")
        .map(|value| {
            content(value, false)
                .into_iter()
                .map(|value| {
                    value
                        .get("text")
                        .cloned()
                        .map_or(value.clone(), |text| json!({ "text": text }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![json!({ "text": "" })]);
    json!({ "toolResult": {
        "toolUseId": block.remove("tool_use_id").unwrap_or(Value::String("tool".into())),
        "content": content,
        "status": if block.remove("is_error").and_then(|v| v.as_bool()).unwrap_or(false) { "error" } else { "success" }
    }})
}

fn reasoning(mut block: Map<String, Value>) -> Value {
    let mut reasoning = Map::new();
    reasoning.insert(
        "text".into(),
        block
            .remove("thinking")
            .unwrap_or(Value::String(String::new())),
    );
    if let Some(signature) = block.remove("signature") {
        reasoning.insert("signature".into(), signature);
    }
    json!({ "reasoningContent": { "reasoningText": reasoning } })
}

fn media(mut block: Map<String, Value>, kind: &str) -> Option<Value> {
    let Value::Object(mut source) = block.remove("source")? else {
        return None;
    };
    if source.get("type").and_then(Value::as_str) != Some("base64") {
        return None;
    }
    let media_type = source.remove("media_type")?.as_str()?.to_owned();
    let format = media_type
        .rsplit('/')
        .next()
        .unwrap_or("png")
        .replace("jpg", "jpeg");
    let bytes = source.remove("data")?;
    if kind == "image" {
        Some(json!({ "image": { "format": format, "source": { "bytes": bytes } } }))
    } else {
        Some(
            json!({ "document": { "format": format, "name": "document", "source": { "bytes": bytes } } }),
        )
    }
}
