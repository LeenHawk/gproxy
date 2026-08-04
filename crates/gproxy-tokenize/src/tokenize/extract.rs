//! Protocol-agnostic text harvesting from provider-native request JSON.
//! Compiled on every target (the edge estimate path needs it too).

use serde_json::Value;

/// Keys whose string values are human text worth counting.
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
/// Keys whose non-string values (tool defs, structured system) are counted
/// by serializing the whole subtree.
const SERIALIZE_KEYS: &[&str] = &[
    "tools",
    "tool_choice",
    "system",
    "response_format",
    "json_schema",
    "schema",
    "generation_config",
];
/// Keys whose array length approximates the message count.
const MESSAGE_KEYS: &[&str] = &["messages", "contents", "input"];

/// Harvest human-text from any provider-native request JSON: walks the value,
/// collecting strings under text-ish keys (`text`, `content`, `instructions`,
/// string-form `system`, gemini parts text) plus tool definitions serialized.
/// Returns `(texts, message_count)` where `message_count` is the length of
/// the largest `messages` / `contents` / `input` array found (0 if none).
pub fn harvest(body: &[u8]) -> (Vec<String>, u64) {
    try_harvest(body).unwrap_or_default()
}

/// Fallible harvesting for callers that must distinguish malformed JSON from
/// an intentionally empty request.
pub fn try_harvest(body: &[u8]) -> Result<(Vec<String>, u64), serde_json::Error> {
    let root = serde_json::from_slice::<Value>(body)?;
    let mut texts = Vec::new();
    let mut messages = 0u64;
    walk(&root, &mut texts, &mut messages);
    Ok((texts, messages))
}

fn walk(value: &Value, texts: &mut Vec<String>, messages: &mut u64) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                match val {
                    Value::String(s) if TEXT_KEYS.contains(&key.as_str()) => {
                        texts.push(s.clone());
                    }
                    _ if SERIALIZE_KEYS.contains(&key.as_str()) && !val.is_null() => {
                        texts.push(val.to_string());
                    }
                    Value::Array(arr) => {
                        if MESSAGE_KEYS.contains(&key.as_str()) {
                            *messages = (*messages).max(arr.len() as u64);
                        }
                        for item in arr {
                            walk(item, texts, messages);
                        }
                    }
                    Value::Object(_) => walk(val, texts, messages),
                    _ => {}
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                walk(item, texts, messages);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{harvest, try_harvest};

    #[test]
    fn harvest_claude_body() {
        let body = serde_json::json!({
            "model": "claude-sonnet-4",
            "system": "be terse",
            "messages": [
                { "role": "user", "content": "hello there" },
                { "role": "assistant", "content": [
                    { "type": "text", "text": "hi!" }
                ]}
            ],
            "tools": [{ "name": "get_weather", "description": "weather" }]
        })
        .to_string();
        let (texts, messages) = harvest(body.as_bytes());
        assert_eq!(messages, 2);
        assert!(texts.iter().any(|t| t == "hello there"));
        assert!(texts.iter().any(|t| t == "hi!"));
        assert!(texts.iter().any(|t| t == "be terse"));
        assert!(texts.iter().any(|t| t.contains("get_weather")));
    }

    #[test]
    fn harvest_openai_responses_string_input() {
        let body = serde_json::json!({
            "model": "deepseek-v4-flash",
            "input": "Count these words."
        })
        .to_string();
        let (texts, messages) = harvest(body.as_bytes());
        assert_eq!(texts, ["Count these words."]);
        assert_eq!(messages, 0);
    }

    #[test]
    fn invalid_json_is_explicit_on_fallible_path() {
        assert!(try_harvest(br#"{"messages":["#).is_err());
        assert_eq!(harvest(br#"{"messages":["#), (Vec::new(), 0));
    }

    #[test]
    fn harvests_reasoning_arguments_and_schema() {
        let body = serde_json::json!({
            "messages": [{
                "role": "assistant",
                "reasoning_content": "think",
                "tool_calls": [{"function": {"arguments": "{\"city\":\"Paris\"}"}}]
            }],
            "response_format": {"type": "json_schema", "json_schema": {"name": "answer"}}
        });
        let (texts, _) = try_harvest(body.to_string().as_bytes()).unwrap();
        assert!(texts.iter().any(|text| text == "think"));
        assert!(texts.iter().any(|text| text.contains("Paris")));
        assert!(texts.iter().any(|text| text.contains("json_schema")));
    }
}
