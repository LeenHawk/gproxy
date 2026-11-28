//! Reasoning on the way in: the effort a client asked for, and the signed
//! reasoning it carries back from an earlier turn.
//!
//! Effort rides in the envelope's top-level `additionalModelRequestFields`,
//! whose shape depends on the model family (Kiro docs, `models/effort`):
//! Claude 4.6 and later take `{thinking: {type: "adaptive"}, output_config:
//! {effort}}`, GPT-5.6 takes `{reasoning: {effort}}`. Claude 4.5 and older
//! refuse the field and `auto` routes to a model the request cannot know, so
//! neither gets it. The field is only written when the client sent
//! `reasoning.effort`; nothing is filled in for a client that did not.
//!
//! The upstream signs its reasoning (`reasoningContentEvent.signature`, or a
//! `redactedContent` blob instead of text) and verifies the signature when the
//! history replays it. The signature is not Anthropic's, so it travels in the
//! Responses reasoning item's `encrypted_content` under this channel's own
//! prefix: to the host it is ciphertext only this upstream reads. Reasoning
//! without one cannot be replayed (the upstream answers 400) and is dropped.

use serde_json::{Value, json};

/// `encrypted_content` of a reasoning item with signed text.
pub(in crate::channels::kiro) const SIGNED: &str = "kiro:";
/// `encrypted_content` of a reasoning item the upstream redacted.
pub(in crate::channels::kiro) const REDACTED: &str = "kiro-redacted:";

/// Effort levels from weakest to strongest (Kiro docs).
const RANK: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
const CLAUDE_4_6: &[&str] = &["low", "medium", "high", "max"];
const CLAUDE_LATER: &[&str] = &["low", "medium", "high", "xhigh", "max"];
const GPT_5_6: &[&str] = &["none", "low", "medium", "high", "xhigh", "max"];

/// The envelope's `additionalModelRequestFields` for this request, if the
/// client asked for an effort the model takes.
pub(super) fn model_fields(value: &Value, model: &str) -> Option<Value> {
    let requested = value
        .get("reasoning")
        .and_then(|reasoning| reasoning.get("effort"))
        .and_then(Value::as_str)?;
    let model = model.to_ascii_lowercase();
    if model.starts_with("gpt-5.6") || model.starts_with("gpt-5-6") {
        let effort = clamp(requested, GPT_5_6)?;
        return Some(json!({"reasoning": {"effort": effort}}));
    }
    let effort = clamp(requested, claude_levels(&model)?)?;
    Some(json!({"thinking": {"type": "adaptive"}, "output_config": {"effort": effort}}))
}

/// The levels a Claude model takes, or None when it takes no effort field.
fn claude_levels(model: &str) -> Option<&'static [&'static str]> {
    if model.contains("fable") {
        return Some(CLAUDE_LATER);
    }
    let rest = model.strip_prefix("claude-")?;
    let (_, version) = rest.split_once('-')?;
    let version = version.split('-').next()?;
    let (major, minor) = version.split_once('.').unwrap_or((version, "0"));
    let version = (major.parse::<u32>().ok()?, minor.parse::<u32>().ok()?);
    match version {
        (4, 6) => Some(CLAUDE_4_6),
        version if version > (4, 6) => Some(CLAUDE_LATER),
        _ => None,
    }
}

/// The requested level if the model takes it, else the nearest stronger one
/// it takes (the strongest when none is stronger). OpenAI's `minimal` reads
/// as `low`; a level with no strength (`none` where it is not offered, or an
/// unknown word) sends nothing.
fn clamp(requested: &str, allowed: &[&'static str]) -> Option<&'static str> {
    let requested = requested.trim().to_ascii_lowercase();
    let requested = if requested == "minimal" {
        "low"
    } else {
        requested.as_str()
    };
    if let Some(level) = allowed.iter().find(|level| **level == requested) {
        return Some(level);
    }
    let wanted = RANK.iter().position(|level| *level == requested)?;
    let rank = |level: &&str| RANK.iter().position(|ranked| ranked == level);
    allowed
        .iter()
        .filter(|level| rank(level).is_some_and(|rank| rank >= wanted))
        .min_by_key(|level| rank(level))
        .or_else(|| allowed.iter().max_by_key(|level| rank(level)))
        .copied()
}

/// The `reasoningContent` an assistant history turn replays for a Responses
/// reasoning item, or None when the item carries no signature of ours.
pub(super) fn history(item: &Value) -> Option<Value> {
    let encrypted = item.get("encrypted_content").and_then(Value::as_str)?;
    if let Some(data) = encrypted.strip_prefix(REDACTED) {
        return (!data.is_empty()).then(|| json!({"redactedContent": data}));
    }
    let signature = encrypted.strip_prefix(SIGNED).filter(|value| !value.is_empty())?;
    let text = texts(item.get("content")).or_else(|| texts(item.get("summary")))?;
    Some(json!({"reasoningText": {"text": text, "signature": signature}}))
}

fn texts(parts: Option<&Value>) -> Option<String> {
    let text = parts?
        .as_array()?
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

