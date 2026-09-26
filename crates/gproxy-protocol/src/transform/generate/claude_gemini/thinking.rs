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
