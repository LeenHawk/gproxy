use bytes::Bytes;
use serde_json::{Map, Value, json};

use super::content::content;

pub(super) fn convert(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice(body.as_ref()) else {
        return body;
    };
    Bytes::from(convert_value(value).to_string())
}

pub(super) fn convert_value(value: Value) -> Value {
    let Value::Object(mut input) = value else {
        return value;
    };
    let mut output = Map::new();
    if let Some(system) = input.remove("system") {
        let blocks = content(system, false);
        if !blocks.is_empty() {
            output.insert("system".into(), Value::Array(blocks));
        }
    }
    if let Some(Value::Array(messages)) = input.remove("messages") {
        output.insert(
            "messages".into(),
            Value::Array(messages.into_iter().filter_map(message).collect()),
        );
    }
    let mut inference = Map::new();
    move_field(&mut input, &mut inference, "max_tokens", "maxTokens");
    move_field(&mut input, &mut inference, "temperature", "temperature");
    move_field(&mut input, &mut inference, "top_p", "topP");
    move_field(
        &mut input,
        &mut inference,
        "stop_sequences",
        "stopSequences",
    );
    if !inference.is_empty() {
        output.insert("inferenceConfig".into(), Value::Object(inference));
    }
    if let Some(Value::Array(tools)) = input.remove("tools") {
        let tools: Vec<_> = tools.into_iter().flat_map(tool).collect();
        if !tools.is_empty() {
            let mut config = Map::new();
            config.insert("tools".into(), Value::Array(tools));
            if let Some(choice) = input.remove("tool_choice").and_then(tool_choice) {
                config.insert("toolChoice".into(), choice);
            }
            output.insert("toolConfig".into(), Value::Object(config));
        }
    }
    let mut additional = Map::new();
    for key in ["thinking", "output_config", "top_k"] {
        if let Some(value) = input.remove(key) {
            additional.insert(key.into(), value);
        }
    }
    if !additional.is_empty() {
        output.insert(
            "additionalModelRequestFields".into(),
            Value::Object(additional),
        );
    }
    Value::Object(output)
}

fn message(value: Value) -> Option<Value> {
    let Value::Object(mut message) = value else {
        return None;
    };
    let role = message.remove("role")?.as_str()?.to_owned();
    let role = if role == "assistant" {
        "assistant"
    } else {
        "user"
    };
    let blocks = content(message.remove("content")?, role == "assistant");
    (!blocks.is_empty()).then(|| json!({ "role": role, "content": blocks }))
}

fn tool(value: Value) -> Vec<Value> {
    let Value::Object(mut tool) = value else {
        return Vec::new();
    };
    let cached = tool.remove("cache_control").is_some();
    let Some(name) = tool.remove("name") else {
        return Vec::new();
    };
    let mut spec = Map::new();
    spec.insert("name".into(), name);
    if let Some(description) = tool.remove("description") {
        spec.insert("description".into(), description);
    }
    spec.insert(
        "inputSchema".into(),
        json!({ "json": tool.remove("input_schema").unwrap_or_else(|| json!({"type":"object"})) }),
    );
    let mut output = vec![json!({ "toolSpec": spec })];
    if cached {
        output.push(json!({ "cachePoint": { "type": "default" } }));
    }
    output
}

fn tool_choice(value: Value) -> Option<Value> {
    let Value::Object(mut choice) = value else {
        return None;
    };
    match choice.remove("type")?.as_str()? {
        "auto" => Some(json!({ "auto": {} })),
        "any" => Some(json!({ "any": {} })),
        "tool" => Some(json!({ "tool": { "name": choice.remove("name")? } })),
        _ => None,
    }
}

fn move_field(
    input: &mut Map<String, Value>,
    output: &mut Map<String, Value>,
    from: &str,
    to: &str,
) {
    if let Some(value) = input.remove(from) {
        output.insert(to.into(), value);
    }
}
