//! The request hygiene the CLI's own requests satisfy, applied to client
//! Messages bodies so the OAuth surface accepts them: content canonicalized
//! to block arrays, empty text blocks removed with their `cache_control`
//! re-attached to the nearest cacheable block, thinking blocks kept only on
//! assistant turns, sampling parameters and trailing assistant prefills
//! removed for models that reject them, feature betas derived from the body,
//! and the CLI's billing system block plus `metadata.user_id`
//! (v3 `shared/claude/{cache,hygiene}.rs` and `claudecode/cch.rs`). The
//! canonicalize/empty-block pass lives in `channels::shared::cache`, where the
//! magic cache strings that build on it live.

use crate::channels::shared::cache;
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

/// `magic_cache` places `cache_control` at the magic cache strings; without
/// it the strings are only stripped.
pub(super) fn messages(body: &mut Value, headers: &mut HeaderMap, magic_cache: bool) {
    if has_fallback_credit(body) {
        return;
    }
    if magic_cache {
        cache::apply(body, cache::Rules::Claude);
    } else {
        // Strip first so a text block that only trailed a token trims the
        // same way a token-free one does.
        cache::strip_tokens(body);
        cache::sanitize_claude(body);
    }
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

/// How long a session's last upstream `request-id` is remembered for
/// `cc_prev_req`.
pub(super) const PREV_REQ_TTL_MS: i64 = 60 * 60 * 1000;

const BILLING_PREFIX: &str = "x-anthropic-billing-header:";

/// The CLI's `x-anthropic-billing-header` system block and the
/// `metadata.user_id` JSON the backend correlates it with (v3 `cch.rs`;
/// captured in `samples/claude-code-2.1.252/messages-oauth-wire.json`).
///
/// The block is rebuilt in the CLI's own order (2.1.284):
/// `cc_version` (ours, with the computed suffix), `cc_entrypoint` (the
/// client's, default `cli`), `cch=00000`, then the optional fragments the
/// client sent when they pass the CLI's own validation: `cc_workload`,
/// `cc_is_subagent=true`, `cc_prev_req`, `cc_prompt_id`, `cc_turn_origin`,
/// then the paired `cc_prompt_index` / `cc_turn_index`. Unknown keys and
/// invalid values are dropped; they would only fingerprint the proxy. Two
/// fragments the CLI always sends on the OAuth path are synthesized when the
/// client left them out: `cc_prev_req` from `prev_req`, the `request-id` of
/// the session's previous upstream call, and `cc_prompt_id`, a UUID derived
/// from the device, the session and the count of user text turns, so it
/// stays fixed across the tool loop of one prompt and changes with the next
/// user message; this approximates the CLI's per-prompt id. `cc_workload`
/// and `cc_is_subagent` stay absent unless sent: absence is the main session.
pub(super) fn inject_billing(
    body: &mut Value,
    device_id: &str,
    account_uuid: &str,
    session: &str,
    prev_req: Option<&str>,
) {
    if has_fallback_credit(body) {
        return;
    }
    let suffix = version_suffix(first_user_text(body));
    let prompt_index = user_text_turns(body);
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
            .is_some_and(|text| text.starts_with(BILLING_PREFIX))
    });
    let sent = existing
        .and_then(|index| blocks[index].get("text"))
        .and_then(Value::as_str)
        .map(billing_fields)
        .unwrap_or_default();
    let field = |name: &str| {
        sent.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    };

    let entrypoint = field("cc_entrypoint")
        .filter(|value| is_token(value))
        .unwrap_or("cli");
    let mut text = format!(
        "{BILLING_PREFIX} cc_version={}.{suffix}; cc_entrypoint={entrypoint}; cch=00000;",
        super::CLI_VERSION,
    );
    if let Some(workload) = field("cc_workload").filter(|value| is_token(value)) {
        text.push_str(&format!(" cc_workload={workload};"));
    }
    if field("cc_is_subagent") == Some("true") {
        text.push_str(" cc_is_subagent=true;");
    }
    if let Some(request) = field("cc_prev_req")
        .filter(|value| is_request_id(value))
        .or_else(|| prev_req.filter(|value| is_request_id(value)))
    {
        text.push_str(&format!(" cc_prev_req={request};"));
    }
    let prompt = match field("cc_prompt_id").filter(|value| is_uuid(value)) {
        Some(prompt) => prompt.to_owned(),
        None => super::derived_uuid(&format!(
            "claudecode-prompt:{device_id}:{session}:{prompt_index}"
        )),
    };
    text.push_str(&format!(" cc_prompt_id={prompt};"));
    if let Some(origin) = field("cc_turn_origin").filter(|value| is_turn_origin(value)) {
        text.push_str(&format!(" cc_turn_origin={origin};"));
    }
    // CLI 2.1.284 emits both indices or neither. These are client session
    // counters: prompt advances only for human input, turn for every new
    // triggering input (not tool round-trips). Preserve, never infer them.
    if let Some(prompt) = field("cc_prompt_index").and_then(|value| billing_index(value, 0))
        && let Some(turn) = field("cc_turn_index").and_then(|value| billing_index(value, 1))
        && prompt <= turn
    {
        text.push_str(&format!(" cc_prompt_index={prompt}; cc_turn_index={turn};"));
    }

    let billing = json!({"type": "text", "text": text});
    match existing {
        Some(index) => blocks[index] = billing,
        None => blocks.insert(0, billing),
    }
}

/// The `key=value;` fragments of a billing block, in order.
fn billing_fields(text: &str) -> Vec<(&str, &str)> {
    text.strip_prefix(BILLING_PREFIX)
        .unwrap_or(text)
        .split(';')
        .filter_map(|field| field.trim().split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .filter(|(key, value)| !key.is_empty() && !value.is_empty())
        .collect()
}

/// The CLI serializes integer counters in decimal, bounded by 10 million.
fn billing_index(value: &str, minimum: u32) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value
        .parse::<u32>()
        .ok()
        .filter(|value| (minimum..=10_000_000).contains(value))
}

/// `[A-Za-z0-9_-]+`, the CLI's charset for entrypoints and workloads.
fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// CLI 2.1.280: `^[a-z][a-z_]{0,31}$`.
fn is_turn_origin(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

/// `^req_[A-Za-z0-9_-]{1,36}$` (2.1.252 `KUt`).
pub(super) fn is_request_id(value: &str) -> bool {
    value
        .strip_prefix("req_")
        .is_some_and(|rest| (1..=36).contains(&rest.len()) && is_token(rest))
}

/// `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i`.
fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// How many user turns carry text (a string, or a `text` block) rather than
/// only tool results: the ordinal of the prompt the request belongs to.
fn user_text_turns(body: &Value) -> usize {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return 0;
    };
    messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .filter(|message| match message.get("content") {
            Some(Value::String(text)) => !text.is_empty(),
            Some(Value::Array(blocks)) => blocks
                .iter()
                .any(|block| block.get("type").and_then(Value::as_str) == Some("text")),
            _ => false,
        })
        .count()
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
