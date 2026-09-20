//! Magic cache strings (v3 `shared/cache.rs`, `shared/openai/cache.rs`,
//! `shared/claude/cache.rs`).
//!
//! A client that cannot set cache breakpoints itself embeds one of three
//! tokens in any text it sends. Wherever a token appears it is removed, and
//! when the provider enables magic cache for that dialect a breakpoint is
//! placed at that spot: Claude gets `cache_control` (`ephemeral`, with the
//! `ttl` the token names), OpenAI gets `prompt_cache_breakpoint`, string
//! contents being split into parts so the marker has somewhere to sit. At
//! most four breakpoints leave the proxy, counting the client's own; extra
//! tokens are stripped without a marker.
//!
//! When magic cache is disabled the tokens are still stripped (a leaked token
//! in a prompt is never wanted) but nothing else is touched: the client's own
//! `cache_control` / `prompt_cache_breakpoint` stay, and a body without a
//! token is forwarded byte for byte. The bare strip pass is what claude.ai,
//! which has no cache control, applies.

use serde_json::Value;

/// Common prefix of the three tokens; a body without it needs no parsing.
#[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
const MAGIC_PREFIX: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_";
const MAGIC_TRIGGER_AUTO_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_TRIGGER_5M_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
const MAGIC_TRIGGER_1H_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";

/// Token → the Claude `ttl` it selects (`None` is the API default).
const MAGIC: &[(&str, Option<&str>)] = &[
    (MAGIC_TRIGGER_AUTO_ID, None),
    (MAGIC_TRIGGER_5M_ID, Some("5m")),
    (MAGIC_TRIGGER_1H_ID, Some("1h")),
];

/// Whether a raw body can contain a token at all.
#[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
pub(crate) fn contains_token(bytes: &[u8]) -> bool {
    let prefix = MAGIC_PREFIX.as_bytes();
    bytes.len() >= prefix.len() && bytes.windows(prefix.len()).any(|window| window == prefix)
}

/// Remove every token from every string in `body`. Returns whether any was
/// found.
pub(crate) fn strip_tokens(body: &mut Value) -> bool {
    let mut found = false;
    match body {
        Value::String(text) => found = strip(text).0,
        Value::Array(values) => {
            for value in values {
                // Every value is visited; no short-circuit on the first hit.
                found |= strip_tokens(value);
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                found |= strip_tokens(value);
            }
        }
        _ => {}
    }
    found
}

/// Strip the tokens out of `text` in place: whether one was present and the
/// `ttl` the first matching token selects.
fn strip(text: &mut String) -> (bool, Option<&'static str>) {
    let mut ttl = None;
    let mut matched = false;
    for (token, token_ttl) in MAGIC {
        if text.contains(token) {
            *text = text.replace(token, "");
            ttl = ttl.or(*token_ttl);
            matched = true;
        }
    }
    (matched, ttl)
}

#[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
pub(crate) use rules::*;

#[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
mod rules {
    use super::*;
    use gproxy_protocol::{Dialect, connection::Bytes};
    use serde_json::{Map, json};

    /// Breakpoints a body may carry in total, the client's own included.
    const MAX_BREAKPOINTS: usize = 4;

    /// The breakpoint shape a body takes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Rules {
        /// Messages bodies: `cache_control` on the text block.
        Claude,
        /// Chat Completions bodies: `prompt_cache_breakpoint` on a part.
        OpenAiChat,
        /// Responses bodies: instructions anchor, prompt variables, input.
        OpenAiResponses,
    }

    /// The rules a dialect follows when its family's magic cache is enabled;
    /// `None` when it is disabled or the dialect has no cache breakpoints.
    pub(crate) fn rules_for(dialect: Dialect, claude: bool, openai: bool) -> Option<Rules> {
        match dialect {
            Dialect::Claude if claude => Some(Rules::Claude),
            Dialect::OpenAiChat if openai => Some(Rules::OpenAiChat),
            Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket if openai => {
                Some(Rules::OpenAiResponses)
            }
            _ => None,
        }
    }

    /// Shape a buffered request body. A body without a token, or one that is
    /// not a JSON object, is returned untouched; otherwise the tokens are
    /// stripped and, with `rules`, breakpoints placed.
    pub(crate) fn shape(bytes: Bytes, rules: Option<Rules>) -> Bytes {
        if !contains_token(&bytes) {
            return bytes;
        }
        let Some(mut body) = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
        else {
            return bytes;
        };
        match rules {
            Some(rules) => apply(&mut body, rules),
            None => {
                strip_tokens(&mut body);
            }
        }
        Bytes::from(body.to_string())
    }

    /// Place breakpoints at the tokens, then strip whatever tokens remain
    /// (beyond the cap, or in fields that take no breakpoint).
    pub(crate) fn apply(body: &mut Value, rules: Rules) {
        match rules {
            Rules::Claude => claude(body),
            Rules::OpenAiChat => openai_chat(body),
            Rules::OpenAiResponses => openai_responses(body),
        }
        strip_tokens(body);
    }

    fn breakpoint_budget(body: &Value, marker: &str) -> usize {
        MAX_BREAKPOINTS.saturating_sub(count(body, marker))
    }

    // ---------------------------------------------------------------- Claude

    /// Canonicalize, mark the text blocks that carry a token, then sanitize
    /// again so empty blocks left by a lone token hand their `cache_control`
    /// to the nearest cacheable block. `system` is walked before `messages`
    /// so the budget is spent in prompt order, not in key order.
    fn claude(body: &mut Value) {
        sanitize_claude(body);
        let mut remaining = breakpoint_budget(body, "cache_control");
        let Some(root) = body.as_object_mut() else {
            return;
        };
        for field in ["system", "messages"] {
            let Some(value) = root.get_mut(field) else {
                continue;
            };
            visit_maps(value, &mut |map| {
                let Some(Value::String(text)) = map.get_mut("text") else {
                    return;
                };
                let (matched, ttl) = strip(text);
                if !matched {
                    return;
                }
                if remaining > 0 && !map.contains_key("cache_control") {
                    map.insert(
                        "cache_control".into(),
                        ttl.map_or_else(
                            || json!({"type": "ephemeral"}),
                            |ttl| json!({"type": "ephemeral", "ttl": ttl}),
                        ),
                    );
                    remaining -= 1;
                }
            });
        }
        sanitize_claude(body);
    }

    /// Canonicalize `system` and message content to block arrays, drop empty
    /// text blocks and move their `cache_control` onto the nearest cacheable
    /// block, and keep thinking blocks only on assistant turns
    /// (v3 `shared/claude/cache.rs`).
    pub(crate) fn sanitize_claude(body: &mut Value) {
        canonicalize(body);
        let Some(root) = body.as_object_mut() else {
            return;
        };
        if let Some(Value::Array(blocks)) = root.remove("system") {
            let blocks = clean_blocks(blocks, &mut [], Scope::System);
            if !blocks.is_empty() {
                root.insert("system".into(), Value::Array(blocks));
            }
        }
        let Some(Value::Array(messages)) = root.remove("messages") else {
            return;
        };
        let mut kept = Vec::with_capacity(messages.len());
        for mut message in messages {
            let Some(map) = message.as_object_mut() else {
                kept.push(message);
                continue;
            };
            let role = map.get("role").and_then(Value::as_str).map(str::to_owned);
            let Some(Value::Array(blocks)) = map.remove("content") else {
                kept.push(message);
                continue;
            };
            let blocks = clean_blocks(blocks, &mut kept, Scope::Message(role.as_deref()));
            if !blocks.is_empty() {
                map.insert("content".into(), Value::Array(blocks));
                kept.push(message);
            }
        }
        root.insert("messages".into(), Value::Array(kept));
    }

    #[derive(Clone, Copy)]
    enum Scope<'a> {
        System,
        Message(Option<&'a str>),
    }

    fn clean_blocks(
        blocks: Vec<Value>,
        previous_messages: &mut [Value],
        scope: Scope<'_>,
    ) -> Vec<Value> {
        let mut kept = Vec::with_capacity(blocks.len());
        for block in blocks {
            let Value::Object(mut map) = block else {
                kept.push(block);
                continue;
            };
            let kind = map.get("type").and_then(Value::as_str);
            if matches!(kind, Some("thinking" | "redacted_thinking"))
                && !matches!(scope, Scope::Message(Some("assistant")))
            {
                continue;
            }
            if kind == Some("text") {
                let trimmed = map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(str::to_owned);
                if let Some(text) = trimmed {
                    if text.is_empty() {
                        if let Some(control) = map.remove("cache_control")
                            && !attach(&mut kept, &control, scope)
                        {
                            attach_to_messages(previous_messages, &control);
                        }
                        continue;
                    }
                    map.insert("text".into(), Value::String(text));
                }
            }
            kept.push(Value::Object(map));
        }
        kept
    }

    fn attach(blocks: &mut [Value], control: &Value, scope: Scope<'_>) -> bool {
        for block in blocks.iter_mut().rev() {
            let Some(map) = block.as_object_mut() else {
                continue;
            };
            let cacheable = match scope {
                Scope::System => cacheable(map),
                Scope::Message(role) => message_cacheable(role, map),
            };
            if cacheable {
                map.entry("cache_control")
                    .or_insert_with(|| control.clone());
                return true;
            }
        }
        false
    }

    fn attach_to_messages(messages: &mut [Value], control: &Value) {
        for message in messages.iter_mut().rev() {
            let Some(map) = message.as_object_mut() else {
                continue;
            };
            let role = map.get("role").and_then(Value::as_str).map(str::to_owned);
            let Some(blocks) = map.get_mut("content").and_then(Value::as_array_mut) else {
                continue;
            };
            if attach(blocks, control, Scope::Message(role.as_deref())) {
                return;
            }
        }
    }

    fn cacheable(block: &Map<String, Value>) -> bool {
        match block.get("type").and_then(Value::as_str) {
            Some("thinking" | "redacted_thinking" | "citation" | "citations") => false,
            Some("char_location" | "page_location" | "content_block_location") => false,
            Some("text") => block
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty()),
            _ => true,
        }
    }

    fn message_cacheable(role: Option<&str>, block: &Map<String, Value>) -> bool {
        cacheable(block)
            && (!matches!(
                block.get("type").and_then(Value::as_str),
                Some("image" | "document")
            ) || role == Some("user"))
    }

    fn canonicalize(body: &mut Value) {
        let Some(root) = body.as_object_mut() else {
            return;
        };
        if let Some(system) = root.get_mut("system") {
            content(system);
        }
        if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
            for message in messages {
                if let Some(content_value) = message
                    .as_object_mut()
                    .and_then(|message| message.get_mut("content"))
                {
                    content(content_value);
                }
            }
        }
    }

    fn content(value: &mut Value) {
        match value {
            Value::String(text) => {
                let text = std::mem::take(text);
                *value = Value::Array(vec![json!({"type": "text", "text": text})]);
            }
            Value::Object(_) => {
                let block = std::mem::take(value);
                *value = Value::Array(vec![block]);
            }
            Value::Array(blocks) => {
                for block in blocks {
                    if let Value::String(text) = block {
                        let text = std::mem::take(text);
                        *block = json!({"type": "text", "text": text});
                    }
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- OpenAI

    /// Chat Completions: a string `content` carrying a token becomes one
    /// marked `text` part; parts carrying a token are marked in place.
    fn openai_chat(body: &mut Value) {
        let mut remaining = breakpoint_budget(body, "prompt_cache_breakpoint");
        let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
            return;
        };
        for message in messages {
            let Some(message) = message.as_object_mut() else {
                continue;
            };
            let supported = !matches!(
                message.get("role").and_then(Value::as_str),
                Some("function") | None
            );
            let Some(content) = message.get_mut("content") else {
                continue;
            };
            if supported && let Some(text) = take_string(content, &mut remaining) {
                *content = Value::Array(vec![marked("text", "text", text)]);
            } else if let Value::Array(parts) = content {
                for part in parts {
                    mark_part(part, true, &mut remaining);
                }
            }
        }
    }

    /// Responses: a token in `instructions` anchors a marked developer
    /// message in front of `input`; prompt variables and input items are
    /// marked like Chat parts, with the `input_text` / `output_text` kinds.
    fn openai_responses(body: &mut Value) {
        let mut remaining = breakpoint_budget(body, "prompt_cache_breakpoint");
        let Some(root) = body.as_object_mut() else {
            return;
        };
        let instruction_triggered = match root.get_mut("instructions") {
            Some(Value::String(text)) => strip(text).0,
            _ => false,
        };
        if instruction_triggered && remaining > 0 && prepend_instruction_anchor(root) {
            remaining -= 1;
        }
        if let Some(variables) = root
            .get_mut("prompt")
            .and_then(Value::as_object_mut)
            .and_then(|prompt| prompt.get_mut("variables"))
            .and_then(Value::as_object_mut)
        {
            for value in variables.values_mut() {
                if let Some(text) = take_string(value, &mut remaining) {
                    *value = marked("input_text", "text", text);
                } else {
                    mark_part(value, false, &mut remaining);
                }
            }
        }
        let Some(input) = root.get_mut("input") else {
            return;
        };
        if let Some(text) = take_string(input, &mut remaining) {
            *input = Value::Array(vec![message("user", text, true)]);
            return;
        }
        let Value::Array(items) = input else {
            return;
        };
        for item in items {
            let Some(item) = item.as_object_mut() else {
                continue;
            };
            let role = item.get("role").and_then(Value::as_str).map(str::to_owned);
            for field in ["content", "output"] {
                let Some(content) = item.get_mut(field) else {
                    continue;
                };
                if let Some(text) = take_string(content, &mut remaining) {
                    let kind = if role.as_deref() == Some("assistant") {
                        "output_text"
                    } else {
                        "input_text"
                    };
                    *content = Value::Array(vec![marked(kind, "text", text)]);
                } else if let Value::Array(parts) = content {
                    for part in parts {
                        mark_part(part, false, &mut remaining);
                    }
                }
            }
        }
    }

    /// A string carrying a token, stripped and taken when a breakpoint may
    /// still be placed on it. The token is stripped in place either way.
    fn take_string(value: &mut Value, remaining: &mut usize) -> Option<String> {
        let Value::String(text) = value else {
            return None;
        };
        if !strip(text).0 || text.trim().is_empty() || *remaining == 0 {
            return None;
        }
        *remaining -= 1;
        Some(std::mem::take(text))
    }

    fn mark_part(part: &mut Value, chat: bool, remaining: &mut usize) {
        let Some(part) = part.as_object_mut() else {
            return;
        };
        let text_key = match (chat, part.get("type").and_then(Value::as_str)) {
            (true, Some("text")) => "text",
            (true, Some("refusal")) => "refusal",
            (false, Some("input_text" | "output_text")) => "text",
            (false, Some("refusal")) => "refusal",
            _ => return,
        };
        let already_marked = part.contains_key("prompt_cache_breakpoint");
        let Some(Value::String(text)) = part.get_mut(text_key) else {
            return;
        };
        if !strip(text).0 {
            return;
        }
        let blank = text.trim().is_empty();
        if !blank && *remaining > 0 && !already_marked {
            part.insert("prompt_cache_breakpoint".into(), breakpoint());
            *remaining -= 1;
        }
    }

    fn prepend_instruction_anchor(root: &mut Map<String, Value>) -> bool {
        let mut items = match root.remove("input") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::String(text)) => vec![message("user", text, false)],
            Some(Value::Array(items)) => items,
            Some(other) => {
                root.insert("input".into(), other);
                return false;
            }
        };
        items.insert(0, message("developer", " ".into(), true));
        root.insert("input".into(), Value::Array(items));
        true
    }

    fn message(role: &str, text: String, marked_part: bool) -> Value {
        let kind = if role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        let mut part = json!({"type": kind, "text": text});
        if marked_part {
            part["prompt_cache_breakpoint"] = breakpoint();
        }
        json!({"type": "message", "role": role, "content": [part]})
    }

    fn marked(kind: &str, text_key: &str, text: String) -> Value {
        let mut value = json!({"type": kind, "prompt_cache_breakpoint": breakpoint()});
        value[text_key] = Value::String(text);
        value
    }

    fn breakpoint() -> Value {
        json!({"mode": "explicit"})
    }

    // --------------------------------------------------------------- walking

    fn visit_maps(value: &mut Value, visit: &mut impl FnMut(&mut Map<String, Value>)) {
        match value {
            Value::Array(values) => {
                for value in values {
                    visit_maps(value, visit);
                }
            }
            Value::Object(map) => {
                visit(map);
                for value in map.values_mut() {
                    visit_maps(value, visit);
                }
            }
            _ => {}
        }
    }

    fn count(value: &Value, name: &str) -> usize {
        match value {
            Value::Array(values) => values.iter().map(|value| count(value, name)).sum(),
            Value::Object(map) => {
                usize::from(map.contains_key(name))
                    + map.values().map(|value| count(value, name)).sum::<usize>()
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const AUTO: &str = MAGIC_TRIGGER_AUTO_ID;
    const FIVE_M: &str = MAGIC_TRIGGER_5M_ID;
    const ONE_H: &str = MAGIC_TRIGGER_1H_ID;

    #[test]
    fn tokens_are_stripped_from_every_string_and_select_a_ttl() {
        let mut text = format!("before {AUTO} middle {ONE_H} after");
        assert_eq!(strip(&mut text), (true, Some("1h")));
        assert_eq!(text, "before  middle  after");
        let mut plain = "nothing here".to_owned();
        assert_eq!(strip(&mut plain), (false, None));
        let mut both = format!("{FIVE_M}{ONE_H}");
        assert_eq!(strip(&mut both), (true, Some("5m")), "first token wins");

        let mut body = json!({
            "system": format!("sys {AUTO}"),
            "messages": [{"role": "user", "content": [{"type": "text", "text": format!("hi {FIVE_M}")}]}],
            "tools": [{"description": format!("tool {ONE_H}")}],
            "n": 1
        });
        assert!(strip_tokens(&mut body));
        assert_eq!(
            body,
            json!({
                "system": "sys ",
                "messages": [{"role": "user", "content": [{"type": "text", "text": "hi "}]}],
                "tools": [{"description": "tool "}],
                "n": 1
            })
        );
        assert!(!strip_tokens(&mut body));
    }

    #[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
    #[test]
    fn claude_breakpoints_carry_the_ttl_and_respect_the_cap_with_client_marks() {
        let mut body = json!({
            "system": format!("policy {ONE_H}"),
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "kept", "cache_control": {"type": "ephemeral"}},
                    {"type": "text", "text": format!("a {FIVE_M}")},
                    {"type": "text", "text": format!("b {AUTO}")},
                    {"type": "text", "text": format!("c {AUTO}")},
                    {"type": "text", "text": format!("d {AUTO}")}
                ]}
            ]
        });
        apply(&mut body, Rules::Claude);
        assert_eq!(
            body["system"],
            json!([{"type": "text", "text": "policy", "cache_control": {"type": "ephemeral", "ttl": "1h"}}]),
            "string system canonicalized and marked with the token's ttl"
        );
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["cache_control"], json!({"type": "ephemeral"}));
        assert_eq!(
            blocks[1]["cache_control"],
            json!({"type": "ephemeral", "ttl": "5m"})
        );
        assert_eq!(blocks[2]["cache_control"], json!({"type": "ephemeral"}));
        assert!(
            blocks[3].get("cache_control").is_none() && blocks[4].get("cache_control").is_none(),
            "four in total, the client's own counted: {blocks:?}"
        );
        for block in blocks {
            assert!(!block["text"].as_str().unwrap().contains(MAGIC_PREFIX));
        }
    }

    #[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
    #[test]
    fn claude_lone_token_block_hands_its_mark_to_the_previous_block() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "context"},
                    {"type": "text", "text": AUTO}
                ]},
                {"role": "assistant", "content": "ok"}
            ]
        });
        apply(&mut body, Rules::Claude);
        assert_eq!(
            body["messages"][0]["content"],
            json!([{"type": "text", "text": "context", "cache_control": {"type": "ephemeral"}}])
        );
        assert_eq!(
            body["messages"][1]["content"],
            json!([{"type": "text", "text": "ok"}])
        );
    }

    #[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
    #[test]
    fn chat_splits_string_content_and_marks_parts_up_to_the_cap() {
        let mut body = json!({
            "messages": [
                {"role": "system", "content": format!("rules {AUTO}")},
                {"role": "function", "content": format!("fn {AUTO}")},
                {"role": "user", "content": [
                    {"type": "text", "text": format!("x {AUTO}"), "prompt_cache_breakpoint": {"mode": "explicit"}},
                    {"type": "text", "text": format!("y {AUTO}")},
                    {"type": "image_url", "image_url": {"url": format!("data:{AUTO}")}},
                    {"type": "refusal", "refusal": format!("no {AUTO}")},
                    {"type": "text", "text": format!("z {AUTO}")}
                ]}
            ]
        });
        apply(&mut body, Rules::OpenAiChat);
        assert_eq!(
            body["messages"][0]["content"],
            json!([{"type": "text", "text": "rules ", "prompt_cache_breakpoint": {"mode": "explicit"}}])
        );
        assert_eq!(
            body["messages"][1]["content"], "fn ",
            "function messages take no parts"
        );
        let parts = body["messages"][2]["content"].as_array().unwrap();
        assert_eq!(parts[0]["text"], "x ");
        assert_eq!(
            parts[1]["prompt_cache_breakpoint"],
            json!({"mode": "explicit"})
        );
        assert_eq!(parts[2]["image_url"]["url"], "data:");
        assert_eq!(
            parts[3]["prompt_cache_breakpoint"],
            json!({"mode": "explicit"})
        );
        assert!(
            parts[4].get("prompt_cache_breakpoint").is_none(),
            "fourth mark counting the client's: {parts:?}"
        );
        assert_eq!(parts[4]["text"], "z ");
    }

    #[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
    #[test]
    fn responses_anchor_instructions_and_mark_variables_and_items() {
        let mut body = json!({
            "instructions": format!("be terse {ONE_H}"),
            "prompt": {"id": "p", "variables": {
                "name": format!("Ada {AUTO}"),
                "doc": {"type": "input_text", "text": format!("doc {AUTO}")},
                "file": {"type": "input_file", "file_id": "f"}
            }},
            "input": [
                {"type": "message", "role": "user", "content": format!("hello {AUTO}")},
                {"type": "message", "role": "assistant", "content": format!("hi {AUTO}")},
                {"type": "function_call_output", "call_id": "c", "output": format!("out {AUTO}")}
            ]
        });
        apply(&mut body, Rules::OpenAiResponses);
        assert_eq!(body["instructions"], "be terse ");
        let input = body["input"].as_array().unwrap();
        assert_eq!(
            input[0],
            json!({"type": "message", "role": "developer", "content": [
                {"type": "input_text", "text": " ", "prompt_cache_breakpoint": {"mode": "explicit"}}
            ]})
        );
        assert_eq!(
            body["prompt"]["variables"]["name"],
            json!({"type": "input_text", "text": "Ada ", "prompt_cache_breakpoint": {"mode": "explicit"}})
        );
        assert_eq!(
            body["prompt"]["variables"]["doc"]["prompt_cache_breakpoint"],
            json!({"mode": "explicit"})
        );
        assert_eq!(
            input[1]["content"],
            json!([{"type": "input_text", "text": "hello ", "prompt_cache_breakpoint": {"mode": "explicit"}}])
        );
        assert_eq!(
            input[2]["content"], "hi ",
            "fifth breakpoint over the cap: token stripped, content left as it was"
        );
        assert_eq!(
            input[3]["output"], "out ",
            "beyond the cap the string stays"
        );

        let mut plain = json!({"input": format!("just {FIVE_M}")});
        apply(&mut plain, Rules::OpenAiResponses);
        assert_eq!(
            plain["input"],
            json!([{"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "just ", "prompt_cache_breakpoint": {"mode": "explicit"}}
            ]}])
        );
    }

    #[cfg(any(feature = "codex", feature = "claudecode", feature = "custom"))]
    #[test]
    fn shape_leaves_token_free_and_non_object_bodies_alone() {
        assert!(contains_token(format!("{{\"a\":\"{AUTO}\"}}").as_bytes()));
        assert!(!contains_token(b"{\"a\":\"GPROXY_MAGIC_STRING\"}"));
        let bytes = gproxy_protocol::connection::Bytes::from_static(b"{\"input\":\"x\"}");
        assert_eq!(shape(bytes.clone(), Some(Rules::OpenAiResponses)), bytes);
        let array = gproxy_protocol::connection::Bytes::from(format!("[\"{AUTO}\"]"));
        assert_eq!(shape(array.clone(), Some(Rules::Claude)), array);
        let disabled = shape(
            gproxy_protocol::connection::Bytes::from(format!(
                "{{\"input\":\"a {AUTO}\",\"x\":{{\"prompt_cache_breakpoint\":{{\"mode\":\"explicit\"}}}}}}"
            )),
            None,
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&disabled).unwrap(),
            json!({"input": "a ", "x": {"prompt_cache_breakpoint": {"mode": "explicit"}}})
        );
    }
}
