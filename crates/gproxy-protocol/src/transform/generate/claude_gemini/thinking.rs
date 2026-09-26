//! Gemini thought runs shown to Claude clients under a host-owned handle.
//!
//! A Claude `thinking` block must carry a `signature`, and a Gemini thought
//! carries a `thoughtSignature` that only the same Gemini upstream can verify.
//! The client never receives the native value: a signed run becomes a block
//! signed `gproxy.thinking.v1.<id>`, and the host keeps the native part under
//! `<id>`. Only a later request to the same origin and model gets the native
//! part back; every other path recognises the prefix and drops the block, so a
//! handle never reaches Anthropic or any other upstream.
//!
//! A run is a sequence of text-only thought parts closed by the part that
//! carries the signature (Code Assist streams the thought text first, then an
//! empty thought part holding only the signature). A run that ends without a
//! signature is an unsigned summary: it is shown with the fixed unsigned handle
//! and dropped on replay, since Gemini reads nothing from unsigned thought text.

use crate::wire::gemini as g;

/// Every handle starts with this; nothing else in a Claude request does.
pub const THINKING_HANDLE_PREFIX: &str = "gproxy.thinking.v1.";
const UNSIGNED: &str = "unsigned";

/// True for any signature gproxy issued in place of a native one.
pub fn is_thinking_handle(signature: &str) -> bool {
    signature.starts_with(THINKING_HANDLE_PREFIX)
}

/// The state identity a handle names; `None` for the unsigned handle and for
/// a signature gproxy did not issue.
pub fn thinking_handle_id(signature: &str) -> Option<&str> {
    signature
        .strip_prefix(THINKING_HANDLE_PREFIX)
        .filter(|id| !id.is_empty() && *id != UNSIGNED)
}

pub(crate) fn signed_handle(id: &str) -> String {
    format!("{THINKING_HANDLE_PREFIX}{id}")
}

pub(crate) fn unsigned_handle() -> String {
    format!("{THINKING_HANDLE_PREFIX}{UNSIGNED}")
}

/// A thought part that belongs to a run: thought text, or the empty thought
/// part carrying the run's signature. Thought media keeps its own mapping.
pub(crate) fn is_run_part(part: &g::Part) -> bool {
    part.thought == Some(true)
        && (part.text.is_some() || part.thought_signature.is_some())
        && part.function_call.is_none()
        && part.function_response.is_none()
        && part.inline_data.is_none()
        && part.file_data.is_none()
        && part.executable_code.is_none()
        && part.code_execution_result.is_none()
}

/// An empty visible text part with nothing else: Code Assist opens and closes
/// streams with these. It adds no text, opens no block and does not end a
/// thought run.
pub(crate) fn is_empty_text(part: &g::Part) -> bool {
    part.thought != Some(true)
        && part.text.as_deref() == Some("")
        && part.thought_signature.is_none()
        && part.function_call.is_none()
        && part.function_response.is_none()
        && part.inline_data.is_none()
        && part.file_data.is_none()
        && part.executable_code.is_none()
        && part.code_execution_result.is_none()
}

/// The native part a signed run replays as: all of the run's text and its
/// signature in one thought part, the only shape the upstream accepts back
/// (`index` is the signature part's position among the candidate's parts).
pub(crate) fn signed_run(parts: &[g::Part], index: usize) -> Option<g::Part> {
    let closing = parts.get(index)?;
    if !is_run_part(closing) {
        return None;
    }
    let signature = closing.thought_signature.clone()?;
    let start = parts[..index]
        .iter()
        .rposition(|part| {
            (!is_run_part(part) && !is_empty_text(part)) || part.thought_signature.is_some()
        })
        .map_or(0, |position| position + 1);
    let text: String = parts[start..=index]
        .iter()
        .filter(|part| is_run_part(part))
        .filter_map(|part| part.text.as_deref())
        .collect();
    Some(
        g::Part::builder()
            .thought(true)
            .text(text)
            .thought_signature(signature)
            .build(),
    )
}

/// A Claude Messages (or count tokens) body with every handle-signed
/// `thinking` block removed, and any message the removal empties; `None`
/// when the body holds no handle. This is for a request that reaches an
/// Anthropic upstream as it is, without conversion: the handle means nothing
/// there and Anthropic refuses it as an invalid signature.
pub fn without_thinking_handles(body: &[u8]) -> Option<Vec<u8>> {
    let needle = THINKING_HANDLE_PREFIX.as_bytes();
    if !body.windows(needle.len()).any(|window| window == needle) {
        return None;
    }
    let mut value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let messages = value.get_mut("messages")?.as_array_mut()?;
    let mut changed = false;
    for message in messages.iter_mut() {
        let Some(content) = message
            .get_mut("content")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        let before = content.len();
        content.retain(|block| {
            block.get("type").and_then(serde_json::Value::as_str) != Some("thinking")
                || !block
                    .get("signature")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(is_thinking_handle)
        });
        changed |= content.len() != before;
    }
    if !changed {
        return None;
    }
    messages.retain(|message| {
        message
            .get("content")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|content| !content.is_empty())
    });
    serde_json::to_vec(&value).ok()
}
