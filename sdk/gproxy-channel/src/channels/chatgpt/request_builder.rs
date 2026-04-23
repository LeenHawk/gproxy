//! Transform an OpenAI-shaped request body into chatgpt.com's
//! `/backend-api/f/conversation` wire format.
//!
//! Covers `chat/completions` (messages array) and a minimal subset of
//! `responses` (input array with typed content). Anything richer falls
//! back to best-effort text extraction.

use serde_json::{Value, json};

/// Build the `/f/conversation` request body from an OpenAI request and a
/// resolved upstream model slug.
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
    let hints = extract_system_hints(openai_body, resolved_model);
    body.insert(
        "system_hints".to_string(),
        Value::Array(hints.iter().map(|s| Value::String(s.clone())).collect()),
    );
    body.insert("supports_buffering".to_string(), json!(true));
    body.insert("supported_encodings".to_string(), json!(["v1"]));
    if temporary_chat {
        // "Temporary chat" — exclude this turn from the user's ChatGPT
        // history and from model training (matches the UI toggle).
        body.insert("history_and_training_disabled".to_string(), json!(true));
    }
    if let Some(effort) = extract_thinking_effort(openai_body) {
        body.insert("thinking_effort".to_string(), json!(effort));
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
                    && item_type != "message"
                {
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
/// - Empty / friendly aliases (`gpt-5`, `gpt-5-instant`,
///   `gpt-5-thinking`, `gpt-5-pro`, `gpt-5-latest`, `gpt-5-auto`)
///   resolve to the latest known upstream slug for that variant.
/// - A trailing built-in-tool suffix is **stripped** here (e.g.
///   `gpt-5-thinking@search` → `gpt-5-thinking`); `extract_system_hints`
///   reads the suffix separately. Both `@<tool>` and `:<tool>` separators
///   are accepted.
/// - Anything else passes through verbatim.
pub fn resolve_model(requested: &str) -> String {
    const DEFAULT: &str = "gpt-5-4";
    let trimmed = strip_tool_suffix(requested.trim());
    match trimmed {
        "" | "gpt-5" | "gpt-5-latest" | "gpt-5-auto" => DEFAULT.to_string(),
        "gpt-5-instant" => "gpt-5-3-instant".to_string(),
        "gpt-5-thinking" => "gpt-5-4-thinking".to_string(),
        "gpt-5-pro" => "gpt-5-4-pro".to_string(),
        other => other.to_string(),
    }
}

/// Tool-suffix table — friendly name → upstream `system_hint` id.
///
/// Use as `<model>@<tool>` (or `<model>:<tool>`). For example:
///
/// * `gpt-5-thinking@search` — text + web search
/// * `gpt-5@image`           — auto-generate an image
/// * `gpt-5-pro@deep-research` — deep-research connector
const TOOL_SUFFIXES: &[(&str, &str)] = &[
    ("image", "picture_v2"),
    ("picture", "picture_v2"),
    ("search", "search"),
    ("web", "search"),
    ("study", "tatertot"),
    ("learn", "tatertot"),
    ("agent", "agent"),
    ("canvas", "canvas"),
    ("connectors", "slurm"),
    ("company", "glaux"),
    ("deep-research", "connector:connector_openai_deep_research"),
    ("deepresearch", "connector:connector_openai_deep_research"),
    ("research", "connector:connector_openai_deep_research"),
    ("quiz", "connector:connector_openai_quizgpt_v2"),
    ("quizzes", "connector:connector_openai_quizgpt_v2"),
];

fn split_tool_suffix(slug: &str) -> Option<(&str, &str)> {
    let sep = slug.rfind(|c: char| c == '@' || c == ':')?;
    Some((&slug[..sep], &slug[sep + 1..]))
}

fn strip_tool_suffix(slug: &str) -> &str {
    if let Some((base, suffix)) = split_tool_suffix(slug)
        && TOOL_SUFFIXES.iter().any(|(name, _)| *name == suffix)
    {
        return base;
    }
    slug
}

/// Pull `system_hints` from the request body OR from a model-name
/// suffix.  Sources, in priority order (later sources extend earlier
/// ones, never replace):
///
/// 1. `body.system_hints: ["picture_v2", "search", ...]` — raw upstream ids
/// 2. `body.extra_body.system_hints: [...]` — same
/// 3. `body.extra_body.tools_hint: ["image", "search", ...]` — friendly
///    aliases that we map to upstream ids via [`TOOL_SUFFIXES`]
/// 4. Trailing `@<tool>` / `:<tool>` on the resolved model name
pub fn extract_system_hints(body: &Value, resolved_model: &str) -> Vec<String> {
    let mut hints: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        if !s.is_empty() && !hints.iter().any(|x| x == s) {
            hints.push(s.to_string());
        }
    };

    // Raw upstream ids straight through.
    for arr_path in [
        body.get("system_hints"),
        body.get("extra_body").and_then(|x| x.get("system_hints")),
    ] {
        if let Some(arr) = arr_path.and_then(|v| v.as_array()) {
            for v in arr {
                if let Some(s) = v.as_str() {
                    push(s);
                }
            }
        }
    }

    // Friendly aliases via tools_hint.
    if let Some(arr) = body
        .get("extra_body")
        .and_then(|x| x.get("tools_hint"))
        .and_then(|v| v.as_array())
    {
        for v in arr {
            if let Some(name) = v.as_str()
                && let Some(id) = friendly_to_hint(name)
            {
                push(id);
            }
        }
    }

    // Model-name suffix.
    let raw_requested = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
    if let Some((_, suffix)) = split_tool_suffix(raw_requested.trim())
        && let Some(id) = friendly_to_hint(suffix)
    {
        push(id);
    }

    // Also accept the suffix on the (already-resolved) slug — in case
    // the upstream model itself encodes it.
    if let Some((_, suffix)) = split_tool_suffix(resolved_model)
        && let Some(id) = friendly_to_hint(suffix)
    {
        push(id);
    }

    hints
}

fn friendly_to_hint(name: &str) -> Option<&'static str> {
    TOOL_SUFFIXES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, id)| *id)
}

/// Pull a `thinking_effort` value out of an OpenAI-shaped request body.
///
/// Looked up in this priority order:
/// 1. Top-level `thinking_effort` (chatgpt-web native).
/// 2. `extra_body.thinking_effort` (the standard extra-body escape hatch).
/// 3. `extra_body.reasoning.effort` and `reasoning.effort`
///    (OpenAI Responses-API field). Mapped: `low`→`standard`,
///    `medium`→`extended`, `high`→`max`. Other values pass through
///    unchanged so callers can specify the chatgpt-native names directly.
pub fn extract_thinking_effort(body: &Value) -> Option<String> {
    let raw = body
        .get("thinking_effort")
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("extra_body")
                .and_then(|x| x.get("thinking_effort"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            body.get("extra_body")
                .and_then(|x| x.get("reasoning"))
                .and_then(|r| r.get("effort"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            body.get("reasoning")
                .and_then(|r| r.get("effort"))
                .and_then(|v| v.as_str())
        })?;
    Some(map_effort(raw).to_string())
}

fn map_effort(raw: &str) -> &str {
    match raw {
        "low" => "standard",
        "medium" => "extended",
        "high" => "max",
        other => other,
    }
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

    #[test]
    fn forwards_thinking_effort_top_level() {
        let body = json!({
            "model": "gpt-5-thinking",
            "messages": [{"role": "user", "content": "hi"}],
            "thinking_effort": "max"
        });
        let out = build_conversation_body(&body, "gpt-5-thinking", true);
        assert_eq!(out["thinking_effort"], json!("max"));
    }

    #[test]
    fn maps_responses_api_effort_aliases() {
        let body = json!({
            "model": "gpt-5",
            "messages": [{"role": "user", "content": "hi"}],
            "extra_body": {"reasoning": {"effort": "high"}}
        });
        let out = build_conversation_body(&body, "gpt-5-4", true);
        assert_eq!(out["thinking_effort"], json!("max"));
    }

    #[test]
    fn omits_thinking_effort_when_absent() {
        let body = json!({"messages": [{"role": "user", "content": "hi"}]});
        let out = build_conversation_body(&body, "gpt-5-4", true);
        assert!(out.get("thinking_effort").is_none());
    }

    #[test]
    fn forwards_explicit_system_hints() {
        let body = json!({
            "messages": [{"role": "user", "content": "hi"}],
            "system_hints": ["picture_v2", "search"],
        });
        let out = build_conversation_body(&body, "gpt-5-4", true);
        assert_eq!(out["system_hints"], json!(["picture_v2", "search"]));
    }

    #[test]
    fn maps_tools_hint_aliases() {
        let body = json!({
            "messages": [{"role": "user", "content": "hi"}],
            "extra_body": {"tools_hint": ["image", "search", "deep-research"]},
        });
        let out = build_conversation_body(&body, "gpt-5-4", true);
        assert_eq!(
            out["system_hints"],
            json!([
                "picture_v2",
                "search",
                "connector:connector_openai_deep_research"
            ])
        );
    }

    #[test]
    fn extracts_hint_from_model_suffix() {
        let body = json!({
            "model": "gpt-5-thinking@search",
            "messages": [{"role": "user", "content": "hi"}],
        });
        let resolved = resolve_model("gpt-5-thinking@search");
        assert_eq!(resolved, "gpt-5-4-thinking");
        let out = build_conversation_body(&body, &resolved, true);
        assert_eq!(out["system_hints"], json!(["search"]));
        assert_eq!(out["model"], json!("gpt-5-4-thinking"));
    }

    #[test]
    fn unknown_suffix_is_left_in_model_name() {
        // Unknown suffix → no strip, no hint extraction.
        assert_eq!(resolve_model("gpt-5@bogus"), "gpt-5@bogus");
        let body = json!({
            "model": "gpt-5@bogus",
            "messages": [{"role": "user", "content": "hi"}],
        });
        let out = build_conversation_body(&body, "gpt-5@bogus", true);
        let empty: Vec<String> = Vec::new();
        assert_eq!(out["system_hints"], json!(empty));
    }
}
