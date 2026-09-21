//! The model selector catalogue.
//!
//! `GetChatMessageRequest.model_selector` (#21) is a string selector, not an
//! enum ordinal: both the dash form (`swe-1-6-slow`) and the upstream's
//! enum spelling (`MODEL_SWE_1_5_SLOW`) are accepted, and which of the two a
//! model answers to is not derivable — it was captured from a live
//! `GetCliModelConfigs` response.
//!
//! Two layers make up the catalogue:
//!
//! * **The captured snapshot**, `assets/devin-catalog-snapshot.json`, copied
//!   verbatim from `samples/windsurfapi/src/data/devin-catalog-snapshot.json`:
//!   123 selectors with their provider, `_calibratedAt: 2026-07-08`. It is the
//!   set of selectors #21 is known to accept, including every effort level
//!   (`-low`/`-medium`/`-high`/`-xhigh`/`-max`), every `-fast` and `-priority`
//!   variant and every `-1m` long-context variant. **The snapshot is dated.** A
//!   live `GetCliModelConfigs` is the authority, and this channel does not call
//!   it: the upstream's own list is an account-entitlement view rather than the
//!   set of strings #21 accepts, and calling it would cost a round trip per
//!   request. `config.models` is the escape valve for anything added since.
//! * **The hand-written aliases** below, which map client-facing names onto
//!   selectors. The snapshot carries an `alias` field too, but it is a *family*
//!   alias repeated across every effort variant (seventeen of its aliases name
//!   more than one selector), so it cannot choose a default on its own. These
//!   entries make that choice and are the only place a name like
//!   `claude-sonnet-4.6` acquires a specific target.
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
//!   sent and the upstream's refusal is reported as it arrives. Note that the
//!   snapshot does **not** list `swe-1-6-slow` at all — it was captured on an
//!   account whose entitlement view omitted it — which is why the two layers
//!   are unioned rather than one replacing the other.

use std::{collections::BTreeMap, sync::LazyLock};

use serde::Deserialize;

use crate::channel::ChannelError;

/// The one selector a free-tier account can run
/// (`devin-connect-models.js::FREE_TIER_SELECTOR`). Recorded for operators
/// and for the descriptor; nothing here falls back to it.
pub const FREE_SELECTOR: &str = "swe-1-6-slow";

/// The router selectors. They need an `AssignModel` round trip to resolve to a
/// concrete `model_uid` before a chat call, which this channel does not
/// implement because every one of that method's field numbers is a guess. They
/// are named here only so the refusal can say why rather than reading as a
/// typo.
pub const ROUTER_SELECTORS: &[&str] = &["adaptive"];
const ROUTER_PREFIX: &str = "arena-";

const SNAPSHOT_JSON: &str = include_str!("../../../assets/devin-catalog-snapshot.json");

#[derive(Debug, Deserialize)]
struct Snapshot {
    #[serde(rename = "_count")]
    count: usize,
    #[serde(rename = "_calibratedAt")]
    calibrated_at: String,
    models: Vec<SnapshotModel>,
}

#[derive(Debug, Deserialize)]
struct SnapshotModel {
    selector: String,
    /// The upstream provider family. Reported on `GET /v1/models` as
    /// `owned_by` so a client can tell a Claude selector from a GPT one.
    provider: Option<String>,
}

/// The date the snapshot was taken, for the operator-facing model list.
pub static CALIBRATED_AT: LazyLock<&'static str> =
    LazyLock::new(|| SNAPSHOT.as_ref().map_or("unknown", |s| s.1.as_str()));

/// Parsed once. The error is kept as a string so the static stays
/// `Send + Sync`; an asset that does not parse should fail the same way at
/// every call site rather than at a random one.
#[allow(clippy::type_complexity)]
static SNAPSHOT: LazyLock<Result<(BTreeMap<String, Option<String>>, String), String>> =
    LazyLock::new(parse_snapshot);

fn parse_snapshot() -> Result<(BTreeMap<String, Option<String>>, String), String> {
    let snapshot: Snapshot =
        serde_json::from_str(SNAPSHOT_JSON).map_err(|error| error.to_string())?;
    // The asset's own counter is the cheapest check that it was not truncated
    // or regenerated against a different shape.
    if snapshot.count != snapshot.models.len() {
        return Err(format!(
            "devin catalogue declares {} selectors and carries {}",
            snapshot.count,
            snapshot.models.len()
        ));
    }
    let models = snapshot
        .models
        .into_iter()
        .map(|model| (model.selector, model.provider))
        .collect();
    Ok((models, snapshot.calibrated_at))
}

/// Every captured selector with its provider family.
fn snapshot() -> &'static BTreeMap<String, Option<String>> {
    static EMPTY: LazyLock<BTreeMap<String, Option<String>>> = LazyLock::new(BTreeMap::new);
    match SNAPSHOT.as_ref() {
        Ok((models, _)) => models,
        // A broken asset must not take the channel down: the hand-written
        // aliases below still resolve, and `catalogue()` still answers.
        Err(_) => &EMPTY,
    }
}

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
/// alias table or an entry of the captured snapshot. This is what lets a
/// client send a selector the alias table has no shorthand for — every effort
/// level, `-fast`, `-priority` and `-1m` variant — without the channel
/// inventing one.
fn is_selector(name: &str, extra: &BTreeMap<String, String>) -> bool {
    extra.values().any(|selector| selector == name)
        || SELECTORS.iter().any(|(_, selector)| *selector == name)
        || snapshot().contains_key(name)
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
    if is_router(&normalized) {
        return Err(router(name));
    }
    Err(unknown(name))
}

/// `adaptive` and `arena-*` are routers rather than models.
fn is_router(name: &str) -> bool {
    ROUTER_SELECTORS.contains(&name) || name.starts_with(ROUTER_PREFIX)
}

/// A router selector fails for a reason of its own, so it says so rather than
/// reading as a misspelled model.
fn router(name: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!(
        "devin: `{name}` is a router selector. Resolving one to a concrete \
         model needs an AssignModel round trip, which this channel does not \
         implement because every field number of that method is a guess in \
         both mirrors; sending the router name to #21 fails upstream with an \
         opaque internal error. Name a concrete selector instead."
    ))
}

fn unknown(name: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!(
        "devin: `{name}` is not in the model catalogue, which was captured on \
         {}. Name a catalogued selector, or add it under the provider's \
         `models` configuration — that is where a selector the upstream added \
         since the capture goes. An unknown name is refused rather than \
         answered on the free `{FREE_SELECTOR}` model, which would change \
         both the answer and the billing.",
        *CALIBRATED_AT
    ))
}

/// Every selector a provider can reach, sorted and deduplicated: the captured
/// snapshot, the alias table's targets and whatever the provider configured.
pub fn catalogue(extra: &BTreeMap<String, String>) -> Vec<String> {
    let mut names: Vec<String> = SELECTORS
        .iter()
        .map(|(_, selector)| (*selector).to_owned())
        .chain(snapshot().keys().cloned())
        .chain(extra.values().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// An OpenAI `GET /v1/models` body for the catalogue. `owned_by` reports the
/// snapshot's provider family where it has one, so a client can tell a Claude
/// selector from a GPT one without a table of its own.
pub fn openai_list(extra: &BTreeMap<String, String>) -> serde_json::Value {
    let snapshot = snapshot();
    let data: Vec<serde_json::Value> = catalogue(extra)
        .into_iter()
        .map(|id| {
            let owner = snapshot
                .get(&id)
                .and_then(Option::as_deref)
                .unwrap_or(super::config::ID);
            serde_json::json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": owner,
            })
        })
        .collect();
    serde_json::json!({"object": "list", "data": data})
}
