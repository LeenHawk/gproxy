//! The body and beta-header shaping a Messages request needs to be accepted
//! (v3 `shared/claude/{hygiene,fallback}.rs`).
//!
//! Unlike the Claude Code surface, nothing here impersonates a client: there
//! is no billing block and no `metadata.user_id`. What remains are the
//! corrections the API itself demands — sampling parameters and assistant
//! prefills that newer models refuse — and the two betas whose trigger lives
//! in the body rather than in a header.

use crate::channels::shared::{cache, claude_fallback::fallbacks};
use http::{HeaderMap, HeaderValue};
use serde_json::Value;

use super::claude_fallback::FallbackMode;

const FAST_MODE_BETA: &str = "fast-mode-2026-02-01";
/// The 1M-token context beta is a header the first-party API does not take;
/// a client that sends it would otherwise be refused outright.
const CONTEXT_1M_BETA: &str = "context-1m-2025-08-07";
const THINKING_DISPLAY_UPDATES_BETA: &str = "thinking-display-updates-2026-08-18";
/// Models that still accept `temperature` and `top_k` (v3
/// `shared/claude/hygiene.rs`). Matched as a prefix of the body's `model`.
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

/// Models that still accept an assistant prefill. Matched as a substring, so
/// a namespaced id keeps its meaning.
const PREFILL_TOLERANT: &[&str] = &[
    "claude-3-opus",
    "claude-opus-4-1",
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-haiku-4-5",
];

pub(crate) fn json_object(body: &[u8]) -> Option<Value> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .filter(Value::is_object)
}

/// A request replaying a server-side fallback credit is sent back exactly as
/// it arrived: the credit is bound to the body Anthropic already priced.
fn has_fallback_credit(body: &Value) -> bool {
    body.get("fallback_credit_token")
        .is_some_and(|value| !value.is_null())
}

/// Shape a Messages body. `magic_cache` places `cache_control` at the magic
/// cache strings; without it the strings are only stripped.
pub(crate) fn messages(
    body: &mut Value,
    headers: &mut HeaderMap,
    magic_cache: bool,
    mode: &FallbackMode,
    models: &[String],
) {
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
    betas(body, headers);
    fallbacks(body, headers, mode, models);
}

/// A count_tokens body is only read: the two body-triggered betas still have
/// to be declared, because the count depends on them.
pub(crate) fn count_tokens(body: &Value, headers: &mut HeaderMap) {
    betas(body, headers);
}

/// The OpenAI compatibility layer inherits Anthropic's prefill rule but none
/// of the Messages betas, which it does not read.
#[cfg(feature = "claudeapi")]
pub(crate) fn chat(body: &mut Value) {
    coerce_prefill(body);
}

// -------------------------------------------------------------- sampling

/// `temperature`, `top_p` and `top_k` are refused while extended thinking is
/// on and by models outside `SAMPLING_TOLERANT`. Tolerant models still reject
/// `temperature` and `top_p` together.
fn strip_sampling(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let thinking = root
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
        == Some("enabled");
    let tolerant = !thinking
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

// ------------------------------------------------------------- fallbacks

// ----------------------------------------------------------------- betas

/// The betas whose trigger is in the body, plus the one the first-party API
/// never accepts.
fn betas(body: &Value, headers: &mut HeaderMap) {
    let mut values = beta_values(headers);
    values.retain(|value| value != CONTEXT_1M_BETA);
    if body.get("speed").and_then(Value::as_str) == Some("fast") {
        push(&mut values, FAST_MODE_BETA);
    }
    if body.pointer("/thinking/display").and_then(Value::as_str) == Some("updates") {
        push(&mut values, THINKING_DISPLAY_UPDATES_BETA);
    }
    write_beta(headers, values);
}

fn push(values: &mut Vec<String>, beta: &str) {
    if !values.iter().any(|value| value == beta) {
        values.push(beta.to_owned());
    }
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
