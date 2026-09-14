use super::super::{GeminiReplayContext, GeminiResponseContext, RestoredGeminiPart};
use super::common::{StreamLimits, invalid, limit, measure};
use crate::{transform::TransformError, wire::DeclaredFields};

pub struct GeminiToResponsesContext {
    pub response: GeminiResponseContext,
    /// Actual selected/observed upstream model, never the client's routing alias.
    pub actual_model: Option<String>,
    /// Explicit final thinking observation when native cumulative fields omit the final split.
    pub final_thinking_tokens: Option<i64>,
}
#[derive(Default)]
pub struct ResponsesToGeminiContext {
    pub restoration: GeminiReplayContext,
}
pub(super) fn clean_context(
    mut c: GeminiToResponsesContext,
    limits: StreamLimits,
) -> Result<GeminiToResponsesContext, TransformError> {
    if c.response.created_at < 0
        || c.actual_model.as_ref().is_some_and(|v| v.trim().is_empty())
        || [
            c.response.usage.cache_write_tokens,
            c.response.usage.cached_tokens,
            c.final_thinking_tokens,
        ]
        .into_iter()
        .flatten()
        .any(|v| v < 0)
    {
        return Err(invalid("negative facts or empty actual model"));
    }
    c.response.request = c.response.request.into_declared();
    c.response.effective_tool_choice = c.response.effective_tool_choice.into_declared();
    c.response.effective_prompt_cache_options =
        c.response.effective_prompt_cache_options.into_declared();
    let bytes = measure(&c.response.request, limits.max_bytes)?
        .checked_add(measure(
            &c.response.effective_tool_choice,
            limits.max_bytes,
        )?)
        .and_then(|v| v.checked_add(c.actual_model.as_ref().map_or(0, String::len)))
        .ok_or_else(limit)?;
    if bytes > limits.max_bytes {
        return Err(limit());
    }
    measure(
        &c.response.effective_prompt_cache_options,
        limits.max_bytes.saturating_sub(bytes),
    )?;
    Ok(c)
}
pub(super) fn clone_response(c: &GeminiResponseContext) -> GeminiResponseContext {
    GeminiResponseContext {
        request: c.request.clone(),
        effective_parallel_tool_calls: c.effective_parallel_tool_calls,
        effective_tool_choice: c.effective_tool_choice.clone(),
        usage: c.usage,
        created_at: c.created_at,
        effective_prompt_cache_options: c.effective_prompt_cache_options.clone(),
    }
}
pub(super) fn clean_replay(
    mut c: GeminiReplayContext,
    limits: StreamLimits,
) -> Result<GeminiReplayContext, TransformError> {
    if c.parts.len() > limits.max_items {
        return Err(limit());
    }
    let mut size = measure(&c.target, limits.max_bytes)?;
    for (key, piece) in &mut c.parts {
        piece.part = std::mem::replace(
            &mut piece.part,
            crate::wire::gemini::Part::builder().build(),
        )
        .into_declared();
        size = size.checked_add(key.len()).ok_or_else(limit)?;
        size = size
            .checked_add(measure(
                &piece.state,
                limits.max_bytes.saturating_sub(size),
            )?)
            .ok_or_else(limit)?;
        size = size
            .checked_add(measure(&piece.part, limits.max_bytes.saturating_sub(size))?)
            .ok_or_else(limit)?;
        if size > limits.max_bytes {
            return Err(limit());
        }
    }
    Ok(c)
}
pub(super) fn clone_replay(c: &GeminiReplayContext) -> GeminiReplayContext {
    GeminiReplayContext {
        target: c.target.clone(),
        parts: c
            .parts
            .iter()
            .map(|(id, v)| {
                (
                    id.clone(),
                    RestoredGeminiPart {
                        state: v.state.clone(),
                        part: v.part.clone(),
                    },
                )
            })
            .collect(),
    }
}
