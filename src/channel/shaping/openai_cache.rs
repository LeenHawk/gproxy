//! Explicit prompt-cache breakpoints for OpenAI Chat/Responses request bodies.
//!
//! OpenAI uses content-part `prompt_cache_breakpoint` markers instead of
//! Anthropic's `cache_control`. The same frozen GPROXY magic strings are
//! accepted so clients can use one trigger convention across protocols.

#[path = "openai_cache/helpers.rs"]
mod helpers;
#[path = "openai_cache/magic.rs"]
mod magic;
#[path = "openai_cache/manual.rs"]
mod manual;
#[path = "openai_cache/mutation.rs"]
mod mutation;
#[path = "openai_cache/schema.rs"]
mod schema;
#[path = "openai_cache/selection.rs"]
mod selection;

use serde_json::Value;

use crate::protocol::{ContentGenerationKind, OperationKey, OperationKind};

const MAX_BREAKPOINTS: usize = 4;

/// Return the OpenAI content-generation kind carried by an existing route.
pub fn kind_for_operation(op: OperationKey) -> Option<ContentGenerationKind> {
    match op.kind {
        OperationKind::ContentGeneration(
            kind @ (ContentGenerationKind::OpenAiChatCompletions
            | ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket),
        ) => Some(kind),
        _ => None,
    }
}

/// Strip GPROXY magic strings and stamp explicit OpenAI cache breakpoints.
/// Existing markers remain available for OpenAI's read window; only markers
/// added by this pass count toward its four-new-writes limit.
pub fn apply_magic_string_cache_breakpoints(body: &mut Value, kind: ContentGenerationKind) {
    let mut remaining = MAX_BREAKPOINTS;
    let Some(root) = body.as_object_mut() else {
        return;
    };
    match kind {
        ContentGenerationKind::OpenAiChatCompletions => magic::apply_chat(root, &mut remaining),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => {
            magic::apply_responses(root, &mut remaining)
        }
        _ => {}
    }
}

/// Apply the existing manual `cache_breakpoint` rule to an OpenAI body.
///
/// `system` and `message` select content blocks. OpenAI does not support
/// breakpoints on tool definitions. `top_level`/`global` configures implicit
/// request-wide caching, matching the old target's global-policy semantics.
pub fn apply_manual_cache_breakpoint(
    body: &mut Value,
    kind: ContentGenerationKind,
    target: &str,
    index: Option<i64>,
    ttl: Option<&str>,
) -> Result<(), &'static str> {
    let root = body.as_object_mut().ok_or("body not an object")?;

    if matches!(target, "top_level" | "global") {
        helpers::set_prompt_cache_options(root, Some("implicit"), ttl)?;
        return Ok(());
    }
    if target == "tools" {
        return Err("OpenAI does not support cache breakpoints on tools");
    }

    match kind {
        ContentGenerationKind::OpenAiChatCompletions => manual::apply_chat(root, target, index)?,
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => {
            manual::apply_responses(root, target, index)?
        }
        _ => return Err("non-OpenAI target"),
    }

    if ttl == Some("30m") {
        helpers::set_prompt_cache_options(root, None, ttl)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "openai_cache/tests.rs"]
mod tests;
