//! Gemini thought runs shown to Claude clients, and the reverse on replay.
//!
//! A Claude `thinking` block must carry a `signature`, and a Gemini thought
//! carries a `thoughtSignature` that only a Gemini upstream can verify. A
//! signed run becomes one block whose signature is the native value behind the
//! `gemini:` prefix (see `signature`); a later request to a Gemini upstream
//! turns it back into one thought part with the run's text and signature, and
//! every other path recognises the prefix and drops the block, so a Gemini
//! signature never reaches Anthropic or any other upstream.
//!
//! A run is a sequence of text-only thought parts closed by the part that
//! carries the signature (Code Assist streams the thought text first, then an
//! empty thought part holding only the signature). A run that ends without a
//! signature is an unsigned summary: it is shown with the bare prefix and
//! dropped on replay, since Gemini reads nothing from unsigned thought text.
//! A signature on a function call (Gemini flash models) has no Claude field of
//! its own, so it travels on an empty thinking block right before the
//! `tool_use` it belongs to, behind the `gemini-next:` prefix.

use crate::{transform::generate::signature, wire::gemini as g};

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

/// The signature a `tool_use` built from `part` needs in front of it: set only
/// for a signed function call, which is the only signed non-thought part a
/// Claude client can replay.
pub(crate) fn call_signature(part: &g::Part) -> Option<String> {
    part.function_call
        .as_ref()
        .and(part.thought_signature.as_deref())
        .map(signature::gemini_next)
}

/// A Claude Messages (or count tokens) body with every Gemini-signed
/// `thinking` block removed, and any message the removal empties; `None`
/// when the body holds none. This is for a request that reaches an Anthropic
/// upstream as it is, without conversion: Anthropic refuses a signature it
/// did not issue, which is the case after the conversation moved over from a
/// Gemini upstream.
pub fn without_gemini_thinking(body: &[u8]) -> Option<Vec<u8>> {
    let needles = [signature::GEMINI, signature::GEMINI_NEXT].map(str::as_bytes);
    if !needles
        .iter()
        .any(|needle| body.windows(needle.len()).any(|window| window == *needle))
    {
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
                    .is_some_and(signature::is_gemini)
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
