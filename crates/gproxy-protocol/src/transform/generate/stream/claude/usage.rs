use super::collector::invalid;
use crate::{
    transform::TransformError,
    wire::claude::{
        generate_content as c,
        stream::{MessageDeltaEvent, StreamMessage},
    },
};
pub(super) fn merge(m: &mut StreamMessage, e: MessageDeltaEvent) -> Result<(), TransformError> {
    if let Some(old) = m.stop_reason.flatten()
        && e.delta.stop_reason.is_some_and(|new| new != Some(old))
    {
        return Err(invalid(
            "stop_reason",
            "terminal reason cannot change or clear",
        ));
    }
    if e.delta.stop_reason.is_some() {
        m.stop_reason = e.delta.stop_reason;
    }
    if e.delta.stop_sequence.is_some() {
        m.stop_sequence = e.delta.stop_sequence;
    }
    if e.delta.stop_details.is_some() {
        m.stop_details = e.delta.stop_details;
    }
    if e.delta.container.is_some() {
        m.container = e.delta.container;
    }
    if e.context_management.is_some() {
        m.context_management = e.context_management;
    }
    if let Some(Some(n)) = e.usage.input_tokens {
        m.usage.input_tokens = n;
    }
    if e.usage.output_tokens < m.usage.output_tokens {
        return Err(invalid(
            "usage.output_tokens",
            "cumulative output count decreased",
        ));
    }
    m.usage.output_tokens = e.usage.output_tokens;
    if e.usage.cache_creation_input_tokens.is_some() {
        m.usage.cache_creation_input_tokens = e.usage.cache_creation_input_tokens;
    }
    if e.usage.cache_read_input_tokens.is_some() {
        m.usage.cache_read_input_tokens = e.usage.cache_read_input_tokens;
    }
    if e.usage.fallback_credit.is_some() {
        m.usage.fallback_credit = e.usage.fallback_credit;
    }
    if e.usage.iterations.is_some() {
        m.usage.iterations = e.usage.iterations;
    }
    if e.usage.output_tokens_details.is_some() {
        m.usage.output_tokens_details = e.usage.output_tokens_details;
    }
    if e.usage.server_tool_use.is_some() {
        m.usage.server_tool_use = e.usage.server_tool_use;
    }
    validate(&m.usage)
}
pub(super) fn validate(u: &c::Usage) -> Result<(), TransformError> {
    for n in [
        Some(u.input_tokens),
        Some(u.output_tokens),
        u.cache_creation_input_tokens.flatten(),
        u.cache_read_input_tokens.flatten(),
    ]
    .into_iter()
    .flatten()
    {
        if n < 0 {
            return Err(invalid("usage", "negative token count"));
        }
    }
    if let Some(Some(d)) = &u.output_tokens_details
        && (d.thinking_tokens < 0 || d.thinking_tokens > u.output_tokens)
    {
        return Err(invalid(
            "usage.output_tokens_details",
            "thinking tokens outside output total",
        ));
    }
    if let Some(Some(d)) = &u.server_tool_use
        && (d.web_fetch_requests < 0 || d.web_search_requests < 0)
    {
        return Err(invalid("usage.server_tool_use", "negative tool count"));
    }
    validate_cache(u.cache_creation.as_ref().and_then(Option::as_ref))?;
    if let Some(Some(iterations)) = &u.iterations {
        for iteration in iterations {
            let (created, read, input, output, cache) = match iteration {
                c::IterationUsage::Message(v) => (
                    v.cache_creation_input_tokens,
                    v.cache_read_input_tokens,
                    v.input_tokens,
                    v.output_tokens,
                    v.cache_creation.as_ref().and_then(Option::as_ref),
                ),
                c::IterationUsage::Compaction(v) => (
                    v.cache_creation_input_tokens,
                    v.cache_read_input_tokens,
                    v.input_tokens,
                    v.output_tokens,
                    v.cache_creation.as_ref().and_then(Option::as_ref),
                ),
                c::IterationUsage::Advisor(v) => (
                    v.cache_creation_input_tokens,
                    v.cache_read_input_tokens,
                    v.input_tokens,
                    v.output_tokens,
                    v.cache_creation.as_ref().and_then(Option::as_ref),
                ),
                c::IterationUsage::Fallback(v) => (
                    v.cache_creation_input_tokens,
                    v.cache_read_input_tokens,
                    v.input_tokens,
                    v.output_tokens,
                    v.cache_creation.as_ref().and_then(Option::as_ref),
                ),
            };
            if [created, read, input, output].into_iter().any(|n| n < 0) {
                return Err(invalid(
                    "usage.iterations",
                    "negative iteration token count",
                ));
            }
            validate_cache(cache)?;
        }
    }
    Ok(())
}
fn validate_cache(cache: Option<&c::CacheCreation>) -> Result<(), TransformError> {
    if let Some(cache) = cache
        && (cache.ephemeral_1h_input_tokens < 0
            || cache.ephemeral_5m_input_tokens < 0
            || cache
                .ephemeral_1h_input_tokens
                .checked_add(cache.ephemeral_5m_input_tokens)
                .is_none())
    {
        return Err(invalid(
            "usage.cache_creation",
            "negative or overflowing cache breakdown",
        ));
    }
    Ok(())
}
