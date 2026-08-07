//! Responses `input` replay sanitising for the ChatGPT Codex backend.

use serde_json::{Map, Value};

/// Drop `status` from replayed reasoning items.
///
/// Clients are told to append `response.output` verbatim to the next turn's
/// `input`, but the ChatGPT backend rejects a `status` on a *reasoning* input
/// item with `400 Unknown parameter: 'input[N].status'` — a non-retryable
/// failure that kills a whole tool loop. Other item types accept it.
pub(super) fn strip_reasoning_status(object: &mut Map<String, Value>) {
    let Some(Value::Array(items)) = object.get_mut("input") else {
        return;
    };
    for item in items {
        let is_reasoning = matches!(
            item.get("type").and_then(Value::as_str),
            Some("reasoning" | "compaction")
        );
        if is_reasoning && let Some(object) = item.as_object_mut() {
            object.remove("status");
        }
    }
}
