use crate::{
    transform::TransformError,
    wire::claude::stream::{MessageDeltaEvent, StreamMessage},
};
pub(super) fn merge(m: &mut StreamMessage, e: MessageDeltaEvent) -> Result<(), TransformError> {
    if let Some(old) = m.stop_reason.flatten()
        && e.delta.stop_reason.is_some_and(|new| new != Some(old))
    {
        return Err(super::collector::invalid(
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
    Ok(())
}
