//! Transform an OpenAI-shaped request body into chatgpt.com's
//! `/backend-api/f/conversation` wire format.
//!
//! Covers `chat/completions` (messages array) and a minimal subset of
//! `responses` (input array with typed content). Anything richer falls
//! back to best-effort text extraction.

use serde_json::{Value, json};

/// Build the `/f/conversation` request body from an OpenAI request and a
/// resolved upstream model slug.
///
/// `temporary_chat = true` sets `history_and_training_disabled: true` on
/// the body, mirroring the "Temporary chat" toggle in the chatgpt.com UI.
pub fn build_conversation_body(
    openai_body: &Value,
    resolved_model: &str,
    temporary_chat: bool,
) -> serde_json::Map<String, Value> {
    let messages = extract_messages(openai_body);
    let stream_preview = messages_to_chatgpt(&messages);

    let mut body = serde_json::Map::new();
    body.insert("action".to_string(), json!("next"));
    body.insert("messages".to_string(), Value::Array(stream_preview));
    body.insert(
        "parent_message_id".to_string(),
        json!("client-created-root"),
    );
    body.insert("model".to_string(), json!(resolved_model));
    body.insert("client_prepare_state".to_string(), json!("sent"));
    body.insert("timezone_offset_min".to_string(), json!(-480));
    body.insert("timezone".to_string(), json!("Asia/Shanghai"));
    body.insert(
        "conversation_mode".to_string(),
        json!({ "kind": "primary_assistant" }),
    );
    body.insert("enable_message_followups".to_string(), json!(true));
    body.insert("system_hints".to_string(), Value::Array(vec![]));
    body.insert("supports_buffering".to_string(), json!(true));
    body.insert("supported_encodings".to_string(), json!(["v1"]));
    if temporary_chat {
        // "Temporary chat" — exclude this turn from the user's ChatGPT
        // history and from model training (matches the UI toggle).
        body.insert("history_and_training_disabled".to_string(), json!(true));
    }
    body.insert(
        "client_contextual_info".to_string(),
        json!({
            "is_dark_mode": false,
            "time_since_loaded": 5000,
            "page_height": 1039,
            "page_width": 1237,
            "pixel_ratio": 1.35,
            "screen_height": 1067,
            "screen_width": 1707,
            "app_name": "chatgpt.com"
        }),
    );
    body.insert(
        "paragen_cot_summary_display_override".to_string(),
        json!("allow"),
    );
    body.insert("force_parallel_switch".to_string(), json!("auto"));

    body
}

/// A normalized representation of one OpenAI-style message before we wrap
/// it into chatgpt's message shape.
#[derive(Debug, Clone)]
pub struct NormalizedMessage {
    pub role: String,
    pub text: String,
}

fn extract_messages(openai_body: &Value) -> Vec<NormalizedMessage> {
    // `chat/completions` style: `{messages: [{role, content}, ...]}`.
    if let Some(arr) = openai_body.get("messages").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|m| {
                let role = m.get("role").and_then(|v| v.as_str())?;
                let text = extract_text(m.get("content"))?;
                Some(NormalizedMessage {
                    role: role.to_string(),
                    text,
                })
            })
            .collect();
    }

    // `responses` style: `{input: [...], instructions?}`.
    if let Some(input) = openai_body.get("input") {
        return extract_responses_messages(input, openai_body.get("instructions"));
    }

    // Raw string prompt fallback.
    if let Some(s) = openai_body.get("prompt").and_then(|v| v.as_str()) {
        return vec![NormalizedMessage {
            role: "user".to_string(),
            text: s.to_string(),
        }];
    }

    Vec::new()
}

fn extract_responses_messages(
    input: &Value,
    instructions: Option<&Value>,
) -> Vec<NormalizedMessage> {
    let mut out = Vec::new();
    if let Some(s) = instructions.and_then(|v| v.as_str()) {
        out.push(NormalizedMessage {
            role: "system".to_string(),
            text: s.to_string(),
        });
    }
    match input {
        Value::String(s) => out.push(NormalizedMessage {
            role: "user".to_string(),
            text: s.clone(),
        }),
        Value::Array(arr) => {
            for item in arr {
                if let Some(item_type) = item.get("type").and_then(|v| v.as_str())
                    && item_type != "message" {
                        continue;
                    }
                let role = item
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user")
                    .to_string();
                if let Some(text) = extract_text(item.get("content")) {
                    out.push(NormalizedMessage { role, text });
                }
            }
        }
        _ => {}
    }
    out
}

fn extract_text(content: Option<&Value>) -> Option<String> {
    let content = content?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut buf = String::new();
        for part in arr {
            let text = part
                .get("text")
                .and_then(|v| v.as_str())
                .or_else(|| part.as_str());
            if let Some(t) = text {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(t);
            }
        }
        if buf.is_empty() {
            return None;
        }
        return Some(buf);
    }
    None
}

fn messages_to_chatgpt(messages: &[NormalizedMessage]) -> Vec<Value> {
    // ChatGPT's `/f/conversation` takes a single user turn; history should
    // be flattened into the prompt. Non-assistant prior messages become
    // "<role>: <text>" prefixes; assistant messages (prior replies) can be
    // kept for context too.
    //
    // This is the v1 approach — preserves history faithfully enough for
    // short exchanges. Full multi-turn with correct parent_message_id
    // threading is a follow-up.

    let mut prompt = String::new();
    let mut last_user_only = None;
    for m in messages {
        if m.role == "user" && prompt.is_empty() {
            last_user_only = Some(m.text.clone());
            continue;
        }
        if !prompt.is_empty() {
            prompt.push('\n');
        }
        prompt.push_str(&m.role);
        prompt.push_str(": ");
        prompt.push_str(&m.text);
    }

    let final_prompt = match (prompt.is_empty(), last_user_only) {
        (true, Some(u)) => u,
        (false, Some(u)) => {
            let mut s = prompt;
            s.push_str("\nuser: ");
            s.push_str(&u);
            s
        }
        (_, None) => prompt,
    };

    let msg_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    vec![json!({
        "id": msg_id,
        "author": {"role": "user"},
        "create_time": now,
        "content": {"content_type": "text", "parts": [final_prompt]},
        "metadata": {
            "developer_mode_connector_ids": [],
            "selected_connector_ids": [],
            "selected_sync_knowledge_store_ids": [],
            "selected_sources": [],
            "selected_github_repos": [],
            "selected_all_github_repos": false,
            "serialization_metadata": {"custom_symbol_offsets": []}
        }
    })]
}

/// Resolve an OpenAI model slug to a chatgpt-web-compatible slug.
///
/// - Friendly aliases (`""`, `gpt-5`, `gpt-5-latest`, `gpt-5-auto`)
///   resolve to the current upstream default (`gpt-5-4`).
/// - Models in the curated catalog
///   ([`super::models::known_model_ids`]) pass through unchanged.
/// - Anything else (e.g. a made-up `gpt-5.4`) also falls back to the
///   default so the upstream doesn't 400 on unknown ids.
pub fn resolve_model(requested: &str) -> String {
    const DEFAULT: &str = "gpt-5-4";
    let trimmed = requested.trim();
    match trimmed {
        "" | "gpt-5" | "gpt-5-latest" | "gpt-5-auto" => return DEFAULT.to_string(),
        _ => {}
    }
    if super::models::known_model_ids()
        .iter()
        .any(|id| *id == trimmed)
    {
        return trimmed.to_string();
    }
    DEFAULT.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple_chat_completion() {
        let body = json!({
            "model": "gpt-5",
            "messages": [
                {"role": "user", "content": "hi"}
            ]
        });
        let out = build_conversation_body(&body, &resolve_model("gpt-5"), true);
        assert_eq!(out["model"], json!("gpt-5-4"));
        let msgs = out["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["content"]["parts"][0].as_str().unwrap(), "hi");
    }

    #[test]
    fn extracts_array_content_parts() {
        let body = json!({
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "first"},
                    {"type": "text", "text": "second"}
                ]
            }]
        });
        let out = build_conversation_body(&body, "gpt-5-3", true);
        let text = out["messages"][0]["content"]["parts"][0]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.contains("first"));
        assert!(text.contains("second"));
    }

    #[test]
    fn flattens_multi_turn_history() {
        let body = json!({
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "hello"},
                {"role": "user", "content": "say bye"}
            ]
        });
        let out = build_conversation_body(&body, "gpt-5-3", true);
        let text = out["messages"][0]["content"]["parts"][0]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.contains("system: be brief"));
        assert!(text.contains("assistant: hello"));
        assert!(text.ends_with("user: say bye"));
    }

    #[test]
    fn handles_responses_input_array() {
        let body = json!({
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "responses api"}]
                }
            ]
        });
        let out = build_conversation_body(&body, "gpt-5-3", true);
        let text = out["messages"][0]["content"]["parts"][0]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(text, "responses api");
    }
}
