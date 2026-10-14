//! Live model discovery and backwards-compatible selector resolution.
//!
//! `GetCliModelConfigs` is a raw-protobuf unary RPC. Its enabled, concrete
//! selectors are advertised without adding the historical snapshot. The host
//! merges discoveries into its local, permission-filtered model catalogue.
//!
//! The 2026-07-08 snapshot and legacy aliases below are retained only for name
//! compatibility and effort selection. They are not an admission whitelist:
//! newly discovered selectors pass through unchanged, even after a restart.
//! No unknown name is silently substituted with the free model. Router names
//! still require the unsupported `AssignModel` method and are refused.
//!
//! `config.models` remains an operator-controlled alias override. Listing a
//! model does not establish account entitlement; execution may still be refused.

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

/// Resolve legacy/configured aliases; forward other selectors unchanged.
/// The host owns admission, and new live selectors must survive restarts without
/// depending on this crate's historical snapshot. Never substitute another model.
pub fn resolve(name: &str, extra: &BTreeMap<String, String>) -> Result<String, ChannelError> {
    if name.is_empty() {
        return Err(super::bad_request("devin request has no model"));
    }
    if let Some(selector) = lookup(name, extra) {
        return Ok(selector);
    }
    if is_selector(name, extra) {
        return Ok(name.to_owned());
    }
    if is_router(name) {
        return Err(router(name));
    }
    Ok(name.to_owned())
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

/// Historical effort candidates plus operator overrides. This does not
/// determine the live discovery response or restrict newly discovered names.
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

/// Decode the live unary protobuf response. Fields are wire-calibrated in
/// samples/devin-wire-report.md and windsurfapi/src/devin-connect-catalog.js.
pub(super) fn decode_list(
    bytes: &[u8],
    extra: &BTreeMap<String, String>,
) -> Result<serde_json::Value, ChannelError> {
    use super::proto;
    use serde_json::json;
    let fields = proto::parse(bytes)?;
    let mut data = BTreeMap::new();
    for field in fields.iter().filter(|field| field.number == 1) {
        let proto::Value::Bytes(bytes) = field.value else {
            return Err(ChannelError::InvalidResponse(
                "invalid Devin model entry".into(),
            ));
        };
        let entry = proto::parse(bytes)?;
        let Some(selector) = proto::text_of(&entry, 22).filter(|value| !value.is_empty()) else {
            continue;
        };
        if proto::varint_of(&entry, 4) == Some(1) || is_router(selector) {
            continue;
        }
        let owner = match proto::varint_of(&entry, 10) {
            Some(1) => "cognition",
            Some(2) => "openai",
            Some(3) => "anthropic",
            Some(4) => "google",
            Some(7) => "moonshot",
            Some(9) => "zhipu",
            _ => "devin",
        };
        let mut model = json!({"id":selector,"object":"model","created":0,"owned_by":owner});
        if let Some(label) = proto::text_of(&entry, 1).filter(|value| !value.is_empty()) {
            model["display_name"] = json!(label);
        }
        if let Some(images) = proto::varint_of(&entry, 5) {
            model["input_modalities"] = if images != 0 {
                json!(["text", "image"])
            } else {
                json!(["text"])
            };
        }
        data.insert(selector.to_owned(), model);
    }
    // Operator-declared targets remain available, but never merge the old
    // embedded snapshot into a successful live discovery result.
    for selector in extra.values().filter(|selector| !is_router(selector)) {
        data.entry(selector.clone()).or_insert_with(
            || json!({"id":selector,"object":"model","created":0,"owned_by":"devin"}),
        );
    }
    Ok(json!({"object":"list","data":data.into_values().collect::<Vec<_>>()}))
}

/// Select an effort variant only from known selectors (including configured
/// ones). Never fabricate an upstream model name or silently change families.
pub(super) fn resolve_request(
    name: &str,
    request: &serde_json::Value,
    extra: &BTreeMap<String, String>,
) -> Result<String, ChannelError> {
    const LEVELS: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh", "max"];
    fn split(selector: &str) -> (&str, Option<&str>, &str) {
        let (stem, tail) = ["-priority", "-fast", "-1m"]
            .into_iter()
            .find_map(|tail| selector.strip_suffix(tail).map(|stem| (stem, tail)))
            .unwrap_or((selector, ""));
        if let Some((base, suffix)) =
            stem.rsplit_once(if stem.starts_with("MODEL_") { '_' } else { '-' })
            && let Some(level) = LEVELS
                .iter()
                .copied()
                .chain(["thinking"])
                .find(|level| suffix.eq_ignore_ascii_case(level))
        {
            return (base, Some(level), tail);
        }
        (stem, None, tail)
    }
    let name = name.strip_prefix("devin/").unwrap_or(name);
    let (base, suffix) = name
        .strip_suffix(')')
        .and_then(|n| n.rsplit_once('('))
        .or_else(|| name.rsplit_once(':'))
        .map_or((name, None), |(n, effort)| (n, Some(effort)));
    // A literal effort selector overrides request controls. An operator's alias
    // is also an explicit choice and must not be remapped behind their back.
    if extra.contains_key(base) || (suffix.is_none() && split(base).1.is_some()) {
        return resolve(base, extra);
    }
    let effort = suffix
        .or_else(|| {
            request
                .get("reasoning_effort")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| {
            request
                .pointer("/reasoning/effort")
                .and_then(serde_json::Value::as_str)
        });
    let effort = effort.map(str::to_owned).or_else(|| {
        match request
            .pointer("/thinking/type")
            .and_then(serde_json::Value::as_str)
        {
            Some("disabled") => Some("none".into()),
            Some("adaptive") => Some("high".into()),
            _ => request
                .pointer("/thinking/budget_tokens")
                .and_then(serde_json::Value::as_u64)
                .filter(|budget| *budget > 0)
                .map(|budget| {
                    match budget {
                        0..=4096 => "low",
                        4097..=16384 => "medium",
                        16385..=32768 => "high",
                        _ => "max",
                    }
                    .into()
                }),
        }
    });
    let Some(effort) = effort else {
        return resolve(base, extra);
    };
    let effort = match effort.as_str() {
        "off" | "disabled" => "none",
        "auto" | "adaptive" => "high",
        value => value,
    };
    let rank = LEVELS
        .iter()
        .position(|level| *level == effort)
        .ok_or_else(|| {
            super::bad_request(format!("unsupported Devin reasoning effort `{effort}`"))
        })?;
    let resolved = resolve(base, extra)?;
    // Private Sonnet 4.5 selectors have no effort suffix.
    if matches!(resolved.as_str(), "MODEL_PRIVATE_2" | "MODEL_PRIVATE_3") {
        return Ok(if effort == "none" {
            "MODEL_PRIVATE_2"
        } else {
            "MODEL_PRIVATE_3"
        }
        .into());
    }
    let (family, _, tail) = split(&resolved);
    let candidates = catalogue(extra);
    let selected = candidates
        .iter()
        .filter_map(|candidate| {
            let (other, level, other_tail) = split(candidate);
            if other != family || other_tail != tail {
                return None;
            }
            let index = LEVELS.iter().position(|v| Some(*v) == level)?;
            Some((index, candidate.as_str()))
        })
        .min_by_key(|(index, _)| (index.abs_diff(rank), *index));
    if let Some((_, selector)) = selected {
        return Ok(selector.to_owned());
    }
    // Older Claude families expose a binary thinking/non-thinking pair.
    let thinking = candidates.iter().find(|candidate| {
        let (other, level, other_tail) = split(candidate);
        other == family && other_tail == tail && level == Some("thinking")
    });
    if let Some(thinking) = thinking {
        if effort != "none" {
            return Ok(thinking.clone());
        }
        let plain = format!("{family}{tail}");
        if is_selector(&plain, extra) {
            return Ok(plain);
        }
    }
    Ok(resolved)
}
