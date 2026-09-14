use super::{
    GuardianLimits, GuardianStreamContext, GuardianStreamInvocation, GuardianStreamLimits,
};
use crate::{
    capability::Upstream,
    codec::{self, CodecLimits},
    transform::{
        TransformError, TransformErrorKind,
        guardian::{GuardianDialectRequest, GuardianPreparedRequest},
        identity::IdentityFlow,
    },
    wire::{
        DeclaredFields,
        openai::responses::{generate as g, input as i, response as r},
    },
};
/// Run one prepared review request and return the client's native Responses SSE
/// body. Concrete context contradictions are rejected before upstream send.
pub async fn review_sse<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: GuardianPreparedRequest,
    context: GuardianStreamContext,
    flow: &mut IdentityFlow,
    limits: GuardianLimits,
    stream_limits: GuardianStreamLimits,
) -> Result<GuardianStreamInvocation, TransformError> {
    validate_limits(stream_limits)?;
    let context = preflight(&request, context, stream_limits.codec)?;
    super::review(upstream, target, request, limits)
        .await?
        .into_stream(context, flow, stream_limits)
}
pub async fn classify_sse<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: GuardianPreparedRequest,
    context: GuardianStreamContext,
    flow: &mut IdentityFlow,
    limits: GuardianLimits,
    stream_limits: GuardianStreamLimits,
) -> Result<GuardianStreamInvocation, TransformError> {
    validate_limits(stream_limits)?;
    let context = preflight(&request, context, stream_limits.codec)?;
    super::classify(upstream, target, request, limits)
        .await?
        .into_stream(context, flow, stream_limits)
}
fn preflight(
    request: &GuardianPreparedRequest,
    context: GuardianStreamContext,
    limits: CodecLimits,
) -> Result<GuardianStreamContext, TransformError> {
    if !request.source().stream {
        return Err(TransformError::shape(
            "guardian.stream",
            "prepared client request did not request SSE",
        ));
    }
    if !matches!(
        (request.request(), &context),
        (
            GuardianDialectRequest::OpenAiResponses(_),
            GuardianStreamContext::Responses
        ) | (
            GuardianDialectRequest::OpenAiChat(_),
            GuardianStreamContext::Chat(_)
        ) | (
            GuardianDialectRequest::Claude(_),
            GuardianStreamContext::Claude(_)
        ) | (
            GuardianDialectRequest::Gemini(_),
            GuardianStreamContext::Gemini(_)
        )
    ) {
        return Err(TransformError::shape(
            "guardian.stream.context",
            "prepared dialect and concrete return context differ",
        ));
    }
    Ok(match context {
        GuardianStreamContext::Responses => GuardianStreamContext::Responses,
        GuardianStreamContext::Chat(mut c) => {
            c.request = clean(c.request, limits)?;
            request.validate_response_request(&c.request)?;
            c.effective_tool_choice = c.effective_tool_choice.into_declared();
            c.effective_prompt_cache_options = c.effective_prompt_cache_options.into_declared();
            controls(
                &c.request,
                c.effective_parallel_tool_calls,
                &c.effective_tool_choice,
                c.effective_prompt_cache_options.as_ref(),
            )?;
            counts([
                c.usage.cached_tokens,
                c.usage.cache_write_tokens,
                c.usage.reasoning_tokens,
            ])?;
            GuardianStreamContext::Chat(c)
        }
        GuardianStreamContext::Claude(mut c) => {
            if c.created_at < 0 {
                return Err(TransformError::shape(
                    "guardian.created_at",
                    "negative creation timestamp",
                ));
            }
            c.request = clean(c.request, limits)?;
            request.validate_response_request(&c.request)?;
            c.effective_tool_choice = c.effective_tool_choice.into_declared();
            c.effective_prompt_cache_options = c.effective_prompt_cache_options.into_declared();
            controls(
                &c.request,
                c.effective_parallel_tool_calls,
                &c.effective_tool_choice,
                c.effective_prompt_cache_options.as_ref(),
            )?;
            counts([
                c.usage.cached_tokens,
                c.usage.cache_write_tokens,
                c.usage.reasoning_tokens,
            ])?;
            GuardianStreamContext::Claude(c)
        }
        GuardianStreamContext::Gemini(mut c) => {
            if c.created_at < 0 {
                return Err(TransformError::shape(
                    "guardian.created_at",
                    "negative creation timestamp",
                ));
            }
            c.request = clean(c.request, limits)?;
            request.validate_response_request(&c.request)?;
            c.effective_tool_choice = c.effective_tool_choice.into_declared();
            c.effective_prompt_cache_options = c.effective_prompt_cache_options.into_declared();
            controls(
                &c.request,
                c.effective_parallel_tool_calls,
                &c.effective_tool_choice,
                c.effective_prompt_cache_options.as_ref(),
            )?;
            counts([c.usage.cached_tokens, c.usage.cache_write_tokens, None])?;
            GuardianStreamContext::Gemini(c)
        }
    })
}
fn clean(
    body: g::GenerateContentRequestBody,
    limits: CodecLimits,
) -> Result<g::GenerateContentRequestBody, TransformError> {
    let body = body.into_declared();
    codec::encode_json(&body, limits).map_err(|e| {
        TransformError::new(
            if e.kind() == codec::CodecErrorKind::Limit {
                TransformErrorKind::Limit
            } else {
                TransformErrorKind::InvalidInput
            },
            "guardian.stream.context",
            e.to_string(),
        )
    })?;
    Ok(body)
}
fn counts(values: [Option<i64>; 3]) -> Result<(), TransformError> {
    if values.into_iter().flatten().any(|n| n < 0) {
        Err(TransformError::shape(
            "guardian.usage_facts",
            "negative token count",
        ))
    } else {
        Ok(())
    }
}
fn controls(
    request: &g::GenerateContentRequestBody,
    parallel: bool,
    choice: &i::ToolChoice,
    cache: Option<&r::ResponsePromptCacheOptions>,
) -> Result<(), TransformError> {
    if request
        .parallel_tool_calls
        .flatten()
        .is_some_and(|v| v != parallel)
        || request.tool_choice.as_ref().is_some_and(|v| v != choice)
    {
        return Err(TransformError::shape(
            "guardian.stream.context",
            "effective controls contradict original request",
        ));
    }
    if let Some(requested) = &request.prompt_cache_options {
        let actual = cache
            .ok_or_else(|| TransformError::missing_metadata("guardian.prompt_cache_options"))?;
        if requested.mode.is_some_and(|v| v != actual.mode)
            || requested.ttl.is_some_and(|v| v != actual.ttl)
        {
            return Err(TransformError::shape(
                "guardian.prompt_cache_options",
                "effective cache settings contradict request",
            ));
        }
    }
    Ok(())
}

fn validate_limits(limits: GuardianStreamLimits) -> Result<(), TransformError> {
    if limits.codec.max_body_bytes == 0
        || limits.codec.max_value_bytes == 0
        || limits.codec.max_buffer_bytes == 0
        || limits.codec.max_line_bytes == 0
        || limits.stream.max_events < 8
        || limits.stream.max_items == 0
        || limits.stream.max_bytes == 0
        || limits.stream.max_text_bytes == 0
    {
        return Err(TransformError::new(
            TransformErrorKind::Limit,
            "guardian.sse.limits",
            "limits cannot represent the required Guardian text lifecycle",
        ));
    }
    Ok(())
}
