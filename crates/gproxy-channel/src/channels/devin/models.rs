//! The model selector catalogue.
//!
//! `GetChatMessageRequest.model_selector` (#21) is a string selector, not an
//! enum ordinal: both the dash form (`swe-1-6-slow`) and the upstream's
//! enum spelling (`MODEL_SWE_1_5_SLOW`) are accepted, and which of the two a
//! model answers to is not derivable — it was captured from a live
//! `GetCliModelConfigs` response. This table is ported from that capture as
//! recorded in `samples/windsurfapi/src/devin-connect-models.js`.
//!
//! Two rules travel with it:
//!
//! * **Unknown names are refused, never substituted.** The reference
//!   originally degraded an unmapped name to the free selector and removed
//!   that behaviour once it was understood that answering a paid request on a
//!   free model changes both the answer and the billing
//!   (`docs/DEVIN-CONNECT-CUTOVER.md` §1: a name the catalogue does not know
//!   is rejected). Writing a family alias the catalogue does not list into
//!   #21 is also what makes the upstream fail with an opaque internal error.
//! * **A free account resolves only `swe-1-6-slow`.** Everything else answers
//!   with an upgrade message. That is an account-tier wall rather than a
//!   protocol gap, so this channel does not filter on it: the selector is
//!   sent and the upstream's refusal is reported as it arrives.

use std::collections::BTreeMap;

use crate::channel::ChannelError;

/// The one selector a free-tier account can run
/// (`devin-connect-models.js::FREE_TIER_SELECTOR`). Recorded for operators
/// and for the descriptor; nothing here falls back to it.
pub const FREE_SELECTOR: &str = "swe-1-6-slow";

/// Client-facing name to upstream selector. Both the dotted and dashed
/// spellings are listed because they are not always equivalent: the dotted
/// `claude-sonnet-4.6` is a family alias that resolves to the thinking
/// variant, while the dashed `claude-sonnet-4-6` is itself a catalogue
/// selector for the non-thinking base model.
pub const SELECTORS: &[(&str, &str)] = &[
    // SWE / Cognition. `swe-1-6-slow` is the free-tier selector.
    ("swe-1-6-slow", "swe-1-6-slow"),
    ("swe-1.6-slow", "swe-1-6-slow"),
    ("swe-1-6", "swe-1-6"),
    ("swe-1.6", "swe-1-6"),
    ("swe-1-6-fast", "swe-1-6-fast"),
    ("swe-1.6-fast", "swe-1-6-fast"),
    ("swe-1-5", "MODEL_SWE_1_5_SLOW"),
    ("swe-1.5", "MODEL_SWE_1_5_SLOW"),
    ("swe-1-5-fast", "MODEL_SWE_1_5"),
    ("swe-1.5-fast", "MODEL_SWE_1_5"),
    ("swe-1-7", "swe-1-7"),
    ("swe-1.7", "swe-1-7"),
    ("swe-1-7-lightning", "swe-1-7-lightning"),
    ("swe-1.7-lightning", "swe-1-7-lightning"),
    ("swe-2", "swe-2-medium"),
    ("swe2", "swe-2-medium"),
    ("swe-2-0", "swe-2-medium"),
    ("swe-2.0", "swe-2-medium"),
    ("swe-2-medium", "swe-2-medium"),
    ("swe-2.0-medium", "swe-2-medium"),
    ("swe-2-high", "swe-2-high"),
    ("swe-2.0-high", "swe-2-high"),
    ("swe-2-max", "swe-2-max"),
    ("swe-2.0-max", "swe-2-max"),
    ("subagent-default", "subagent-default"),
    // Anthropic, paid entitlement only.
    ("claude-opus-4-8", "claude-opus-4-8-medium"),
    ("claude-opus-4.8", "claude-opus-4-8-medium"),
    ("claude-opus-4-8-medium", "claude-opus-4-8-medium"),
    ("opus-4-8", "claude-opus-4-8-medium"),
    ("opus-4.8", "claude-opus-4-8-medium"),
    ("claude-opus-4-7", "claude-opus-4-7-medium"),
    ("claude-opus-4.7", "claude-opus-4-7-medium"),
    ("claude-opus-4-6", "claude-opus-4-6"),
    ("claude-opus-4.6", "claude-opus-4-6"),
    ("claude-opus-4-5", "MODEL_CLAUDE_4_5_OPUS"),
    ("claude-opus-4.5", "MODEL_CLAUDE_4_5_OPUS"),
    ("claude-opus-4-5-thinking", "MODEL_CLAUDE_4_5_OPUS_THINKING"),
    ("claude-opus-5", "claude-opus-5-medium"),
    ("claude-sonnet-4-6", "claude-sonnet-4-6"),
    ("claude-sonnet-4.6", "claude-sonnet-4-6-thinking"),
    ("claude-sonnet-4-6-thinking", "claude-sonnet-4-6-thinking"),
    ("claude-sonnet-4-5", "MODEL_PRIVATE_2"),
    ("claude-sonnet-4.5", "MODEL_PRIVATE_2"),
    ("claude-sonnet-4-5-thinking", "MODEL_PRIVATE_3"),
    ("claude-sonnet-5", "claude-sonnet-5-medium"),
    ("claude-haiku-4-5", "MODEL_PRIVATE_11"),
    ("claude-haiku-4.5", "MODEL_PRIVATE_11"),
    ("claude-5-fable", "claude-5-fable-medium"),
    ("claude5", "claude-sonnet-5-medium"),
    // OpenAI, paid entitlement only.
    ("gpt-5-5", "gpt-5-5-low"),
    ("gpt-5.5", "gpt-5-5-low"),
    ("gpt-5-5-low", "gpt-5-5-low"),
    ("gpt-5.5-low", "gpt-5-5-low"),
    ("gpt-5-4", "gpt-5-4-medium"),
    ("gpt-5.4", "gpt-5-4-medium"),
    ("gpt-5-4-mini", "gpt-5-4-mini-medium"),
    ("gpt-5.4-mini", "gpt-5-4-mini-medium"),
    ("gpt-5-3-codex", "gpt-5-3-codex-medium"),
    ("gpt-5.3-codex", "gpt-5-3-codex-medium"),
    ("gpt-5-2", "MODEL_GPT_5_2_NONE"),
    ("gpt-5.2", "MODEL_GPT_5_2_NONE"),
    ("gpt-5-2-low", "MODEL_GPT_5_2_LOW"),
    ("gpt-5-2-medium", "MODEL_GPT_5_2_MEDIUM"),
    ("gpt-5-2-high", "MODEL_GPT_5_2_HIGH"),
    ("gpt-5-2-xhigh", "MODEL_GPT_5_2_XHIGH"),
    ("gpt-5-6-luna", "gpt-5-6-luna-medium"),
    ("gpt-5.6-luna", "gpt-5-6-luna-medium"),
    ("gpt5.6-luna", "gpt-5-6-luna-medium"),
    // Google, paid entitlement only.
    ("gemini-3-flash", "MODEL_GOOGLE_GEMINI_3_0_FLASH_MEDIUM"),
    ("gemini-3-0-flash", "MODEL_GOOGLE_GEMINI_3_0_FLASH_MEDIUM"),
    ("gemini-3.0-flash", "MODEL_GOOGLE_GEMINI_3_0_FLASH_MEDIUM"),
    (
        "gemini-3-flash-minimal",
        "MODEL_GOOGLE_GEMINI_3_0_FLASH_MINIMAL",
    ),
    ("gemini-3-flash-low", "MODEL_GOOGLE_GEMINI_3_0_FLASH_LOW"),
    (
        "gemini-3-flash-medium",
        "MODEL_GOOGLE_GEMINI_3_0_FLASH_MEDIUM",
    ),
    ("gemini-3-flash-high", "MODEL_GOOGLE_GEMINI_3_0_FLASH_HIGH"),
    ("gemini-3-5-flash", "gemini-3-5-flash-medium"),
    ("gemini-3.5-flash", "gemini-3-5-flash-medium"),
    ("gemini-3-1-pro", "gemini-3-1-pro-low"),
    ("gemini-3.1-pro", "gemini-3-1-pro-low"),
    // Others, paid entitlement only.
    ("glm-5-2", "glm-5-2"),
    ("glm-5.2", "glm-5-2"),
    ("glm-5.1", "glm-5-2"),
    ("kimi-k2-7", "kimi-k2-7"),
    ("kimi-k2.7", "kimi-k2-7"),
    ("kimi-k2.6", "kimi-k2-6"),
    ("deepseek-v4", "deepseek-v4"),
];

/// Lowercase, drop a leading `vendor/` prefix some clients prepend, and
/// collapse dots into dashes — the reference's normalization, applied only
/// after an exact match failed so a dotted family alias keeps its own target.
fn normalize(name: &str) -> String {
    let name = name.trim().to_ascii_lowercase();
    let name = match name.split_once('/') {
        Some((vendor, rest)) if !vendor.is_empty() && !rest.is_empty() => rest.to_owned(),
        _ => name,
    };
    name.replace('.', "-")
}

fn lookup(name: &str, extra: &BTreeMap<String, String>) -> Option<String> {
    if let Some(selector) = extra.get(name) {
        return Some(selector.clone());
    }
    SELECTORS
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, selector)| (*selector).to_owned())
}

/// Whether a name is already an upstream selector, i.e. a target of the
/// catalogue. This is what lets a client send a selector the alias table has
/// no shorthand for without the channel inventing one.
fn is_selector(name: &str, extra: &BTreeMap<String, String>) -> bool {
    extra.values().any(|selector| selector == name)
        || SELECTORS.iter().any(|(_, selector)| *selector == name)
}

/// The upstream selector for a client-facing model name, or a refusal.
pub fn resolve(name: &str, extra: &BTreeMap<String, String>) -> Result<String, ChannelError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(unknown(""));
    }
    if let Some(selector) = lookup(name, extra) {
        return Ok(selector);
    }
    if is_selector(name, extra) {
        return Ok(name.to_owned());
    }
    let normalized = normalize(name);
    if let Some(selector) = lookup(&normalized, extra) {
        return Ok(selector);
    }
    if is_selector(&normalized, extra) {
        return Ok(normalized);
    }
    Err(unknown(name))
}

fn unknown(name: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!(
        "devin: `{name}` is not in the model catalogue. Name a catalogued \
         selector, or add it under the provider's `models` configuration; \
         an unknown name is refused rather than answered on the free \
         `{FREE_SELECTOR}` model, which would change both the answer and \
         the billing."
    ))
}

/// Every selector a provider can reach, sorted and deduplicated: the
/// catalogue's targets plus whatever the provider configured.
pub fn catalogue(extra: &BTreeMap<String, String>) -> Vec<String> {
    let mut names: Vec<String> = SELECTORS
        .iter()
        .map(|(_, selector)| (*selector).to_owned())
        .chain(extra.values().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// An OpenAI `GET /v1/models` body for the catalogue.
pub fn openai_list(extra: &BTreeMap<String, String>) -> serde_json::Value {
    let data: Vec<serde_json::Value> = catalogue(extra)
        .into_iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": super::config::ID,
            })
        })
        .collect();
    serde_json::json!({"object": "list", "data": data})
}
