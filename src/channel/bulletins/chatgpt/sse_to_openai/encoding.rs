//! OpenAI chunk encoding and ChatGPT reasoning-value extraction.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChunk {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChunkChoice>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChunkChoice {
    pub index: u32,
    pub delta: serde_json::Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Extract text from a reasoning patch value.
pub(super) fn reasoning_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(object) = value.as_object() {
        for key in ["content", "text", "summary"] {
            if let Some(text) = object
                .get(key)
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Flatten a pre-populated `content.thoughts` array.
pub(super) fn thoughts_array_text(content: Option<&Value>) -> Option<String> {
    let thoughts = content?.get("thoughts")?.as_array()?;
    let mut out = String::new();
    for thought in thoughts {
        for key in ["summary", "content"] {
            if let Some(text) = thought
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(text);
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}
