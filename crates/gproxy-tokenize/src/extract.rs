use serde_json::Value;

const TEXT_KEYS: &[&str] = &[
    "text",
    "content",
    "input",
    "instructions",
    "system",
    "reasoning",
    "reasoning_content",
    "arguments",
    "partial_json",
];
const SERIALIZE_KEYS: &[&str] = &[
    "tools",
    "tool_choice",
    "system",
    "response_format",
    "json_schema",
    "schema",
    "generation_config",
];
const MESSAGE_KEYS: &[&str] = &["messages", "contents", "input"];

pub fn harvest(body: &[u8]) -> (Vec<String>, u64) {
    try_harvest(body).unwrap_or_default()
}

pub fn try_harvest(body: &[u8]) -> Result<(Vec<String>, u64), serde_json::Error> {
    let root = serde_json::from_slice::<Value>(body)?;
    let mut texts = Vec::new();
    let mut messages = 0;
    walk(&root, &mut texts, &mut messages);
    Ok((texts, messages))
}

fn walk(value: &Value, texts: &mut Vec<String>, messages: &mut u64) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                match value {
                    Value::String(text) if TEXT_KEYS.contains(&key.as_str()) => {
                        texts.push(text.clone());
                    }
                    _ if SERIALIZE_KEYS.contains(&key.as_str()) && !value.is_null() => {
                        texts.push(value.to_string());
                    }
                    Value::Array(items) => {
                        if MESSAGE_KEYS.contains(&key.as_str()) {
                            *messages = (*messages).max(items.len() as u64);
                        }
                        for item in items {
                            walk(item, texts, messages);
                        }
                    }
                    Value::Object(_) => walk(value, texts, messages),
                    _ => {}
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, texts, messages);
            }
        }
        _ => {}
    }
}
