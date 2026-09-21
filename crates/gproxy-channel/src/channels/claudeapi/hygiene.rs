//! The body and beta-header shaping a Messages request needs to be accepted
//! (v3 `shared/claude/{hygiene,fallback}.rs`).
//!
//! Unlike the Claude Code surface, nothing here impersonates a client: there
//! is no billing block and no `metadata.user_id`. What remains are the
//! corrections the API itself demands — sampling parameters and assistant
//! prefills that newer models refuse — and the two betas whose trigger lives
//! in the body rather than in a header.

use crate::channels::shared::cache;
use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

use super::FallbackMode;

const FAST_MODE_BETA: &str = "fast-mode-2026-02-01";
/// The 1M-token context beta is a header the first-party API does not take;
/// a client that sends it would otherwise be refused outright.
const CONTEXT_1M_BETA: &str = "context-1m-2025-08-07";
const THINKING_DISPLAY_UPDATES_BETA: &str = "thinking-display-updates-2026-08-18";
/// The beta an explicit fallback chain needs.
const FALLBACK_BETA: &str = "server-side-fallback-2026-06-01";
/// The beta `"fallbacks": "default"` needs.
const DEFAULT_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Anthropic accepts at most three fallback hops.
const MAX_FALLBACKS: usize = 3;

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

/// Models that already fall back server-side, so a `fallbacks` field would
/// only narrow what Anthropic would have done anyway.
const FALLBACK_UNSUPPORTED: &[&str] = &[
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-sonnet-4-6",
    "claude-haiku-4-5",
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-opus-4-1",
    "claude-sonnet-4-0",
    "claude-sonnet-4-20",
    "claude-opus-4-0",
    "claude-opus-4-20",
    "claude-3",
];

pub(super) fn json_object(body: &[u8]) -> Option<Value> {
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
pub(super) fn messages(
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
pub(super) fn count_tokens(body: &Value, headers: &mut HeaderMap) {
    betas(body, headers);
}

/// The OpenAI compatibility layer inherits Anthropic's prefill rule but none
/// of the Messages betas, which it does not read.
pub(super) fn chat(body: &mut Value) {
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

/// Carry the namespace of `model` onto `fallback`, so a prefixed model id
/// (a gateway's `vendor/claude-...`) names a sibling rather than a bare id.
pub(super) fn namespaced(model: &str, fallback: &str) -> String {
    if !fallback.starts_with("claude-") {
        return fallback.to_owned();
    }
    let namespace = model.rfind("claude-").map_or("", |at| &model[..at]);
    format!("{namespace}{fallback}")
}

/// Install the configured fallback chain on a request that names none. A
/// request that already carries `fallbacks` keeps it and only has the matching
/// beta declared.
fn fallbacks(body: &mut Value, headers: &mut HeaderMap, mode: &FallbackMode, models: &[String]) {
    if *mode == FallbackMode::Off {
        return;
    }
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(model) = root.get("model").and_then(Value::as_str).map(str::to_owned) else {
        return;
    };
    if let Some(existing) = root.get("fallbacks").filter(|value| !value.is_null()) {
        let beta = if existing == "default" {
            DEFAULT_FALLBACK_BETA
        } else {
            FALLBACK_BETA
        };
        set_fallback_beta(headers, beta);
        return;
    }
    if FALLBACK_UNSUPPORTED
        .iter()
        .any(|unsupported| model.contains(unsupported))
    {
        return;
    }
    let chain = match mode {
        FallbackMode::Models => {
            let mut chain: Vec<Value> = Vec::new();
            for candidate in models {
                let candidate = namespaced(&model, candidate.trim());
                if candidate.is_empty()
                    || candidate == model
                    || chain.iter().any(|entry| entry["model"] == candidate)
                {
                    continue;
                }
                chain.push(json!({"model": candidate}));
                if chain.len() == MAX_FALLBACKS {
                    break;
                }
            }
            if chain.is_empty() {
                None
            } else {
                Some((Value::Array(chain), FALLBACK_BETA))
            }
        }
        FallbackMode::Default | FallbackMode::Off => None,
    }
    // An empty or fully filtered chain falls back to Anthropic's own.
    .unwrap_or((json!("default"), DEFAULT_FALLBACK_BETA));
    root.insert("fallbacks".into(), chain.0);
    set_fallback_beta(headers, chain.1);
}

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

/// Exactly one server-side fallback beta may be declared; the other is the
/// wrong shape for the `fallbacks` value being sent.
fn set_fallback_beta(headers: &mut HeaderMap, beta: &str) {
    let mut values = beta_values(headers);
    values.retain(|value| value != FALLBACK_BETA && value != DEFAULT_FALLBACK_BETA);
    push(&mut values, beta);
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
