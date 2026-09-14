use super::super::{ClaudeRequestContext, ClaudeResponseContext, RestoredClaudeThinking};
use super::common::{StreamLimits, declared, invalid, measure};
use crate::{
    transform::{Report, TransformError},
    wire::{claude::generate_content as c, openai::responses as r},
};
use std::collections::BTreeMap;
/// Exact source citation occurrence and measured target-native annotation facts.
/// Coordinates and resource IDs are supplied by the host, never guessed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BoundResponseAnnotation {
    pub source_block_index: usize,
    pub source_citation_index: usize,
    pub source: c::ResponseTextCitation,
    pub target: r::OutputAnnotation,
}
pub struct ClaudeToResponsesContext {
    pub response: ClaudeResponseContext,
    pub annotations: Vec<BoundResponseAnnotation>,
}
impl From<ClaudeResponseContext> for ClaudeToResponsesContext {
    fn from(response: ClaudeResponseContext) -> Self {
        Self {
            response,
            annotations: Vec::new(),
        }
    }
}
pub(super) fn clean_context(
    mut ctx: ClaudeToResponsesContext,
    limits: StreamLimits,
) -> Result<ClaudeToResponsesContext, TransformError> {
    ctx.response.request = declared(ctx.response.request);
    ctx.response.effective_tool_choice = declared(ctx.response.effective_tool_choice);
    ctx.response.effective_prompt_cache_options =
        declared(ctx.response.effective_prompt_cache_options);
    if ctx.response.created_at < 0 {
        return Err(invalid("negative factual creation timestamp"));
    }
    for count in [
        ctx.response.usage.cached_tokens,
        ctx.response.usage.cache_write_tokens,
        ctx.response.usage.reasoning_tokens,
    ]
    .into_iter()
    .flatten()
    {
        if count < 0 {
            return Err(invalid("negative factual usage counter"));
        }
    }
    let request = &ctx.response.request;
    if request
        .parallel_tool_calls
        .flatten()
        .is_some_and(|v| v != ctx.response.effective_parallel_tool_calls)
        || request
            .tool_choice
            .as_ref()
            .is_some_and(|v| v != &ctx.response.effective_tool_choice)
    {
        return Err(invalid("effective controls contradict original request"));
    }
    if let Some(requested) = &request.prompt_cache_options {
        let actual = ctx
            .response
            .effective_prompt_cache_options
            .as_ref()
            .ok_or_else(|| TransformError::missing_metadata("prompt_cache_options"))?;
        if requested.mode.is_some_and(|v| v != actual.mode)
            || requested.ttl.is_some_and(|v| v != actual.ttl)
        {
            return Err(invalid(
                "effective cache settings contradict original request",
            ));
        }
    }
    ctx.annotations = ctx
        .annotations
        .into_iter()
        .map(|v| BoundResponseAnnotation {
            source: declared(v.source),
            target: declared(v.target),
            ..v
        })
        .collect();
    measure(
        &(
            &ctx.response.request,
            &ctx.response.effective_tool_choice,
            &ctx.response.effective_prompt_cache_options,
            &ctx.annotations,
        ),
        limits.max_bytes,
    )?;
    Ok(ctx)
}
pub(super) fn clone_response_context(ctx: &ClaudeResponseContext) -> ClaudeResponseContext {
    ClaudeResponseContext {
        request: ctx.request.clone(),
        effective_parallel_tool_calls: ctx.effective_parallel_tool_calls,
        effective_tool_choice: ctx.effective_tool_choice.clone(),
        usage: ctx.usage,
        created_at: ctx.created_at,
        effective_prompt_cache_options: ctx.effective_prompt_cache_options.clone(),
    }
}
pub(super) fn annotations(
    facts: Vec<BoundResponseAnnotation>,
    content: &[c::ResponseContentBlock],
    report: &mut Report,
) -> Result<BTreeMap<usize, Vec<r::OutputAnnotation>>, TransformError> {
    let mut supplied = BTreeMap::new();
    for fact in facts {
        let key = (fact.source_block_index, fact.source_citation_index);
        if supplied.insert(key, fact).is_some() {
            return Err(invalid("duplicate citation binding"));
        }
    }
    let mut output = BTreeMap::new();
    for (block_index, block) in content.iter().enumerate() {
        if let c::ResponseContentBlock::Text(text) = block {
            for (citation_index, citation) in text
                .citations
                .as_ref()
                .and_then(Option::as_ref)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Some(fact) = supplied.remove(&(block_index, citation_index)) {
                    if &fact.source != citation {
                        return Err(invalid(
                            "citation facts belong to a different source occurrence",
                        ));
                    }
                    let (start, end) = match &fact.target {
                        r::OutputAnnotation::Url(v) => (v.start_index, v.end_index),
                        r::OutputAnnotation::ContainerFile(v) => (v.start_index, v.end_index),
                        r::OutputAnnotation::File(v) => (v.index, v.index),
                        r::OutputAnnotation::Path(v) => (v.index, v.index),
                    };
                    if start < 0
                        || end < start
                        || usize::try_from(end)
                            .ok()
                            .is_none_or(|n| n > text.text.len())
                    {
                        return Err(invalid("target citation coordinates exceed source text"));
                    }
                    output
                        .entry(block_index)
                        .or_insert_with(Vec::new)
                        .push(fact.target);
                } else {
                    report.omitted(
                        format!("content[{block_index}].citations[{citation_index}]"),
                        "actual target output coordinates/resource bindings are unavailable",
                    );
                }
            }
        }
    }
    if !supplied.is_empty() {
        return Err(invalid("citation binding has no matching source citation"));
    }
    Ok(output)
}
pub(super) fn clean_restoration(
    context: ClaudeRequestContext,
    limits: StreamLimits,
) -> Result<ClaudeRequestContext, TransformError> {
    if context.restored_thinking.len() > limits.max_items {
        return Err(super::common::limit());
    }
    let context = ClaudeRequestContext {
        target: context.target,
        restored_thinking: context
            .restored_thinking
            .into_iter()
            .map(|(id, v)| {
                (
                    id,
                    RestoredClaudeThinking {
                        state: v.state,
                        block: declared(v.block),
                    },
                )
            })
            .collect(),
    };
    if context.target.is_none() && context.restored_thinking.is_empty() {
        return Ok(context);
    }
    let target = context
        .target
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("thinking target binding"))?;
    if target.dialect != crate::Dialect::Claude
        || target.model.trim().is_empty()
        || target.origin.as_ref().is_none_or(|v| v.trim().is_empty())
    {
        return Err(invalid("invalid signed-thinking origin/model binding"));
    }
    let mut bytes = measure(target, limits.max_bytes)?;
    for (id, value) in &context.restored_thinking {
        value
            .state
            .validate_for(target)
            .map_err(|e| invalid(e.to_string()))?;
        bytes += measure(
            &(id, &value.state, &value.block),
            limits.max_bytes.saturating_sub(bytes),
        )?;
    }
    Ok(context)
}
pub(super) fn clone_restoration(context: &ClaudeRequestContext) -> ClaudeRequestContext {
    ClaudeRequestContext {
        target: context.target.clone(),
        restored_thinking: context
            .restored_thinking
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    RestoredClaudeThinking {
                        state: value.state.clone(),
                        block: value.block.clone(),
                    },
                )
            })
            .collect(),
    }
}
