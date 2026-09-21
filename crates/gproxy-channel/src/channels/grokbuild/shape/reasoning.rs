//! Reasoning items the Grok Build proxy accepts (v3
//! `shape/responses/reasoning.rs`).
//!
//! The proxy validates `encrypted_content` it produced itself and refuses
//! anything else — an OpenAI-shaped blob, a Fernet token (`gAAAA…`), padded
//! base64, or something too short to be a real payload. A reasoning item
//! carrying one of those keeps its summary and loses the blob; a *compaction*
//! item, which is nothing but the blob, is dropped whole. Consecutive
//! summary-only reasoning items are merged, because the proxy rejects a run
//! of them, and `reasoning.effort` is removed for the models that do not
//! take it.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use serde_json::{Map, Value};

/// Shorter than this is not a reasoning payload (v3 `reasoning.rs`).
const MIN_ENCRYPTED_BYTES: usize = 50;

/// The model families that accept a reasoning effort.
const EFFORT_PREFIXES: &[&str] = &["grok-3-mini", "grok-4.20-multi-agent", "grok-4.3"];

pub(super) fn sanitize(object: &mut Map<String, Value>) {
    sanitize_input(object);
    remove_encrypted_include(object);
    strip_unsupported_effort(object);
}

fn sanitize_input(object: &mut Map<String, Value>) {
    let Some(Value::Array(input)) = object.get_mut("input") else {
        return;
    };
    let mut output = Vec::with_capacity(input.len());
    for mut item in std::mem::take(input) {
        let item_kind = kind(&item).to_owned();
        if !matches!(item_kind.as_str(), "reasoning" | "compaction") {
            output.push(item);
            continue;
        }
        let Some(fields) = item.as_object_mut() else {
            output.push(item);
            continue;
        };
        if fields.get("content").is_some_and(Value::is_null) {
            fields.remove("content");
        }
        let invalid = fields
            .get("encrypted_content")
            .is_some_and(|value| !valid_encrypted(value));
        // A compaction item is its blob; without a valid one there is
        // nothing left of it.
        if invalid && item_kind == "compaction" {
            continue;
        }
        if invalid {
            fields.remove("encrypted_content");
        }
        output.push(item);
    }
    *input = merge_summaries(output);
}

fn merge_summaries(items: Vec<Value>) -> Vec<Value> {
    let mut output: Vec<Value> = Vec::with_capacity(items.len());
    for item in items {
        if let Some(previous) = output.last_mut()
            && mergeable(previous, &item)
            && let Some(summary) = item.get("summary").and_then(Value::as_array)
            && let Some(previous) = previous.get_mut("summary").and_then(Value::as_array_mut)
        {
            previous.extend(summary.iter().cloned());
        } else {
            output.push(item);
        }
    }
    output
}

/// Only a bare `{type, summary}` pair folds into the one before it; an item
/// with anything else on it is carrying state that must not be merged away.
fn mergeable(previous: &Value, current: &Value) -> bool {
    kind(previous) == "reasoning"
        && kind(current) == "reasoning"
        && previous.get("summary").is_some_and(Value::is_array)
        && matches!(current.get("summary"), Some(Value::Array(items)) if !items.is_empty())
        && current.as_object().is_some_and(|object| {
            object
                .keys()
                .all(|key| matches!(key.as_str(), "type" | "summary"))
        })
}

fn remove_encrypted_include(object: &mut Map<String, Value>) {
    let Some(include) = object.get("include").and_then(Value::as_array) else {
        return;
    };
    let kept = include
        .iter()
        .filter(|value| value.as_str() != Some("reasoning.encrypted_content"))
        .cloned()
        .collect::<Vec<_>>();
    if kept.is_empty() {
        object.remove("include");
    } else if kept.len() != include.len() {
        object.insert("include".into(), Value::Array(kept));
    }
}

fn strip_unsupported_effort(object: &mut Map<String, Value>) {
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if supports_effort(model) {
        return;
    }
    let Some(reasoning) = object.get_mut("reasoning").and_then(Value::as_object_mut) else {
        return;
    };
    reasoning.remove("effort");
    if reasoning.is_empty() {
        object.remove("reasoning");
    }
}

fn supports_effort(model: &str) -> bool {
    let name = model
        .trim()
        .rsplit_once('/')
        .map_or(model.trim(), |(_, name)| name)
        .to_ascii_lowercase();
    EFFORT_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn valid_encrypted(value: &Value) -> bool {
    let Some(raw) = value.as_str() else {
        return false;
    };
    if raw.is_empty() || raw.trim() != raw || raw.starts_with("gAAAA") || raw.contains('=') {
        return false;
    }
    if !raw
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
    {
        return false;
    }
    STANDARD_NO_PAD
        .decode(raw)
        .is_ok_and(|decoded| decoded.len() >= MIN_ENCRYPTED_BYTES)
}

fn kind(value: &Value) -> &str {
    value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sanitized(value: Value) -> Value {
        let mut object = value.as_object().cloned().unwrap();
        sanitize(&mut object);
        Value::Object(object)
    }

    fn blob() -> String {
        STANDARD_NO_PAD.encode(vec![7_u8; 64])
    }

    #[test]
    fn a_blob_the_proxy_did_not_make_loses_the_item_or_only_the_blob() {
        let out = sanitized(json!({"model": "grok-4", "input": [
            {"type": "reasoning", "summary": [{"type": "summary_text", "text": "a"}],
             "encrypted_content": "gAAAAsomethingelse"},
            {"type": "compaction", "encrypted_content": "gAAAAsomethingelse"},
            {"type": "reasoning", "summary": [], "encrypted_content": blob()},
        ]}));
        let input = out["input"].as_array().unwrap();
        assert_eq!(input.len(), 2, "the compaction item is gone");
        assert!(input[0].get("encrypted_content").is_none());
        assert_eq!(input[0]["summary"][0]["text"], "a");
        assert_eq!(input[1]["encrypted_content"], blob(), "a real one survives");
    }

    #[test]
    fn consecutive_summary_only_items_merge() {
        let out = sanitized(json!({"model": "grok-4", "input": [
            {"type": "reasoning", "summary": [{"text": "a"}]},
            {"type": "reasoning", "summary": [{"text": "b"}]},
            {"type": "reasoning", "summary": [{"text": "c"}], "id": "rs_1"},
        ]}));
        let input = out["input"].as_array().unwrap();
        assert_eq!(input.len(), 2);
        assert_eq!(input[0]["summary"].as_array().unwrap().len(), 2);
        assert_eq!(
            input[1]["id"], "rs_1",
            "an item with state is not merged away"
        );
    }

    #[test]
    fn only_the_families_that_take_an_effort_keep_one() {
        let out = sanitized(json!({"model": "grok-4", "reasoning": {"effort": "high"}}));
        assert!(out.get("reasoning").is_none());
        let out = sanitized(json!({"model": "xai/grok-3-mini", "reasoning": {"effort": "high"}}));
        assert_eq!(out["reasoning"]["effort"], "high");
        let out = sanitized(
            json!({"model": "grok-4", "reasoning": {"effort": "high", "summary": "auto"}}),
        );
        assert_eq!(out["reasoning"], json!({"summary": "auto"}));
    }

    #[test]
    fn asking_for_encrypted_reasoning_back_is_dropped() {
        let out = sanitized(json!({"model": "grok-4",
                                   "include": ["reasoning.encrypted_content", "web_search_call.results"]}));
        assert_eq!(out["include"], json!(["web_search_call.results"]));
        let out = sanitized(json!({"model": "grok-4", "include": ["reasoning.encrypted_content"]}));
        assert!(out.get("include").is_none());
    }
}
