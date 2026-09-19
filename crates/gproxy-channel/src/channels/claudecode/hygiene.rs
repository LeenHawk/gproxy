//! The request hygiene the CLI's own requests satisfy, applied to client
//! Messages bodies so the OAuth surface accepts them: content canonicalized
//! to block arrays, empty text blocks removed with their `cache_control`
//! re-attached to the nearest cacheable block, thinking blocks kept only on
//! assistant turns, sampling parameters and trailing assistant prefills
//! removed for models that reject them, feature betas derived from the body,
//! and the CLI's billing system block plus `metadata.user_id`
//! (v3 `shared/claude/{cache,hygiene}.rs` and `claudecode/cch.rs`).

use http::{HeaderMap, HeaderValue};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const FAST_MODE_BETA: &str = "fast-mode-2026-02-01";
const CONTEXT_1M_BETA: &str = "context-1m-2025-08-07";
const THINKING_DISPLAY_UPDATES_BETA: &str = "thinking-display-updates-2026-08-18";
/// Salt of the CLI's `cc_version` build suffix (v3 `cch.rs`).
const SUFFIX_SALT: &str = "59cf53e54c78";

/// Models that accept `temperature`/`top_k` (v3 `shared/claude/hygiene.rs`).
const SAMPLING_TOLERANT: &[&str] = &[
    "claude-sonnet-4-6",
    "claude-haiku-4-5",
    "claude-sonnet-4-5",
    "claude-opus-4-5",
    "claude-opus-4-1",
    "claude-sonnet-4-0",
    "claude-sonnet-4-20",
    "claude-opus-4-0",
    "claude-opus-4-20",
    "claude-3-opus",
    "claude-3-haiku",
];

/// Models that still accept an assistant prefill.
const PREFILL_TOLERANT: &[&str] = &[
    "claude-3-opus",
    "claude-opus-4-1",
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-haiku-4-5",
];

pub(super) fn json_object(body: &[u8]) -> Option<Value> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .filter(Value::is_object)
}

/// A body carrying a server-side fallback credit is replayed verbatim.
fn has_fallback_credit(body: &Value) -> bool {
    body.get("fallback_credit_token")
        .is_some_and(|value| !value.is_null())
}

pub(super) fn messages(body: &mut Value, headers: &mut HeaderMap) {
    if has_fallback_credit(body) {
        return;
    }
    sanitize_cache(body);
    strip_sampling(body);
    coerce_prefill(body);
    append_fast_beta(body, headers);
    append_thinking_display_beta(body, headers);
    strip_beta(headers, CONTEXT_1M_BETA);
}

pub(super) fn count_tokens(body: &Value, headers: &mut HeaderMap) {
    append_fast_beta(body, headers);
    append_thinking_display_beta(body, headers);
}

// --------------------------------------------------------------- billing

/// The CLI's `x-anthropic-billing-header` system block and the
/// `metadata.user_id` JSON the backend correlates it with (v3 `cch.rs`;
/// captured in `samples/claude-code-2.1.252/messages-oauth-wire.json`).
pub(super) fn inject_billing(body: &mut Value, device_id: &str, account_uuid: &str, session: &str) {
    if has_fallback_credit(body) {
        return;
    }
    let suffix = version_suffix(first_user_text(body));
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let user_id = json!({
        "device_id": device_id,
        "account_uuid": account_uuid,
        "session_id": session,
    })
    .to_string();
    let metadata = root
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    if let Some(metadata) = metadata.as_object_mut() {
        metadata.insert("user_id".into(), Value::String(user_id));
    }

    let system = root
        .entry("system")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !system.is_array() {
        let previous = std::mem::take(system);
        *system = Value::Array(vec![previous]);
    }
    let Some(blocks) = system.as_array_mut() else {
        return;
    };
    let existing = blocks.iter().position(|block| {
        block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.starts_with("x-anthropic-billing-header:"))
    });
    let entrypoint = existing
        .and_then(|index| blocks[index].get("text"))
        .and_then(Value::as_str)
        .and_then(|text| billing_field(text, "cc_entrypoint"))
        .filter(|value| {
            value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .unwrap_or("cli");
    let billing = json!({
        "type": "text",
        "text": format!(
            "x-anthropic-billing-header: cc_version={}.{suffix}; cc_entrypoint={entrypoint}; cch=00000;",
            super::CLI_VERSION,
        ),
    });
    match existing {
        Some(index) => blocks[index] = billing,
        None => blocks.insert(0, billing),
    }
}

fn billing_field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split(';')
        .map(str::trim)
        .find_map(|field| field.strip_prefix(name)?.strip_prefix('='))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn first_user_text(body: &Value) -> &str {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return "";
    };
    for message in messages {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(content) = message.get("content") else {
            continue;
        };
        if let Some(text) = content.as_str() {
            return text;
        }
        if let Some(text) = content.as_array().and_then(|blocks| {
            blocks.iter().find_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| block.get("text").and_then(Value::as_str))
                    .flatten()
            })
        }) {
            return text;
        }
    }
    ""
}

/// Three hex characters from the salt, three UTF-16 units of the first user
/// text and the CLI version (v3 `cch.rs`).
fn version_suffix(text: &str) -> String {
    let code_units = text.encode_utf16().collect::<Vec<_>>();
    let selected = [4_usize, 7, 20]
        .into_iter()
        .map(|index| code_units.get(index).copied().unwrap_or(u16::from(b'0')))
        .collect::<Vec<_>>();
    let selected = String::from_utf16_lossy(&selected);
    let mut hasher = Sha256::new();
    hasher.update(SUFFIX_SALT.as_bytes());
    hasher.update(selected.as_bytes());
    hasher.update(super::CLI_VERSION.as_bytes());
    let digest = hasher.finalize();
    format!("{:02x}{:02x}", digest[0], digest[1])
        .chars()
        .take(3)
        .collect()
}

// -------------------------------------------------------------- sampling

fn strip_sampling(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let tolerant = root
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
        != Some("enabled")
        && root
            .get("model")
            .and_then(Value::as_str)
            .is_some_and(|model| {
                SAMPLING_TOLERANT
                    .iter()
                    .any(|prefix| model.starts_with(prefix))
            });
    if tolerant {
        if root.contains_key("temperature") {
            root.remove("top_p");
        }
    } else {
        for name in ["temperature", "top_p", "top_k"] {
            root.remove(name);
        }
    }
}

/// A trailing assistant text turn is a prefill; models that reject prefills
/// get it as a user turn instead. Thinking or tool blocks are left alone.
fn coerce_prefill(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(model) = root
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
    else {
        return;
    };
    if !model.contains("claude") || PREFILL_TOLERANT.iter().any(|value| model.contains(value)) {
        return;
    }
    let Some(last) = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .and_then(|messages| messages.last_mut())
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let text_prefill = match last.get("content") {
        Some(Value::String(_)) => true,
        Some(Value::Array(blocks)) => {
            !blocks.is_empty()
                && blocks
                    .iter()
                    .all(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        }
        _ => false,
    };
    if text_prefill && last.get("role").and_then(Value::as_str) == Some("assistant") {
        last.insert("role".into(), Value::String("user".into()));
    }
}

// ----------------------------------------------------------------- betas

fn append_fast_beta(body: &Value, headers: &mut HeaderMap) {
    if body.get("speed").and_then(Value::as_str) == Some("fast") {
        append_beta(headers, FAST_MODE_BETA);
    }
}

fn append_thinking_display_beta(body: &Value, headers: &mut HeaderMap) {
    if body.pointer("/thinking/display").and_then(Value::as_str) == Some("updates") {
        append_beta(headers, THINKING_DISPLAY_UPDATES_BETA);
    }
}

fn append_beta(headers: &mut HeaderMap, beta: &str) {
    let mut values = beta_values(headers);
    if !values.iter().any(|value| value == beta) {
        values.push(beta.into());
    }
    write_beta(headers, values);
}

fn strip_beta(headers: &mut HeaderMap, beta: &str) {
    let values = beta_values(headers)
        .into_iter()
        .filter(|value| value != beta)
        .collect();
    write_beta(headers, values);
}

fn beta_values(headers: &HeaderMap) -> Vec<String> {
    headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn write_beta(headers: &mut HeaderMap, values: Vec<String>) {
    if values.is_empty() {
        headers.remove("anthropic-beta");
    } else if let Ok(value) = HeaderValue::from_str(&values.join(",")) {
        headers.insert("anthropic-beta", value);
    }
}

// ----------------------------------------------------------------- cache

/// Canonicalize `system` and message content to block arrays, drop empty
/// text blocks and move their `cache_control` onto the nearest cacheable
/// block, and keep thinking blocks only on assistant turns.
fn sanitize_cache(body: &mut Value) {
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
