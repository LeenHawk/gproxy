use super::super::ResponsesUsageFacts;
use super::common::invalid;
use crate::{
    transform::TransformError,
    wire::{
        claude::{generate_content as c, stream as s},
        openai::responses as r,
    },
};
#[derive(Default)]
pub(super) struct ClaudeProgress {
    output: i64,
    thinking: Option<i64>,
    thinking_at: Option<i64>,
}
impl ClaudeProgress {
    pub fn start(&mut self, v: &c::Usage) -> Result<(), TransformError> {
        self.observe(
            v.output_tokens,
            v.output_tokens_details
                .as_ref()
                .and_then(Option::as_ref)
                .map(|v| v.thinking_tokens),
            v.output_tokens_details.is_some(),
        )
    }
    pub fn delta(&mut self, v: &s::MessageDeltaUsage) -> Result<(), TransformError> {
        self.observe(
            v.output_tokens,
            v.output_tokens_details
                .as_ref()
                .and_then(Option::as_ref)
                .map(|v| v.thinking_tokens),
            v.output_tokens_details.is_some(),
        )
    }
    fn observe(
        &mut self,
        output: i64,
        thinking: Option<i64>,
        present: bool,
    ) -> Result<(), TransformError> {
        if output < self.output {
            return Err(invalid("cumulative output decreased"));
        }
        self.output = output;
        if let Some(n) = thinking {
            if n < 0 || n > output || self.thinking.is_some_and(|old| n < old) {
                return Err(invalid("invalid/decreasing cumulative thinking"));
            }
            self.thinking = Some(n);
            self.thinking_at = Some(output);
        } else if present {
            self.thinking_at = None;
        }
        Ok(())
    }
    pub fn finalize(
        &self,
        v: &mut c::Usage,
        facts: ResponsesUsageFacts,
    ) -> Result<(), TransformError> {
        if facts
            .reasoning_tokens
            .zip(self.thinking)
            .is_some_and(|(new, old)| new < old)
        {
            return Err(invalid("final thinking facts contradict observed prefix"));
        }
        if self.thinking_at != Some(v.output_tokens) {
            v.output_tokens_details = None;
        }
        Ok(())
    }
}
pub(super) fn validate_final(
    initial: &c::Usage,
    final_usage: &c::Usage,
    fixed: bool,
) -> Result<(), TransformError> {
    if final_usage.output_tokens < initial.output_tokens
        || initial
            .output_tokens_details
            .as_ref()
            .and_then(Option::as_ref)
            .zip(
                final_usage
                    .output_tokens_details
                    .as_ref()
                    .and_then(Option::as_ref),
            )
            .is_some_and(|(old, new)| new.thinking_tokens < old.thinking_tokens)
    {
        return Err(invalid(
            "final output/thinking below actual initial observation",
        ));
    }
    if fixed
        && (initial.input_tokens != final_usage.input_tokens
            || initial
                .cache_read_input_tokens
                .flatten()
                .is_some_and(|v| Some(v) != final_usage.cache_read_input_tokens.flatten())
            || initial
                .cache_creation_input_tokens
                .flatten()
                .is_some_and(|v| Some(v) != final_usage.cache_creation_input_tokens.flatten()))
    {
        return Err(invalid(
            "final counters contradict fixed initial input/cache facts",
        ));
    }
    Ok(())
}
pub(super) fn claude_tier(tier: Option<r::ServiceTier>) -> Option<Option<c::ResponseServiceTier>> {
    match tier {
        Some(r::ServiceTier::Default) => Some(Some(c::ResponseServiceTier::Standard)),
        Some(r::ServiceTier::Priority) => Some(Some(c::ResponseServiceTier::Priority)),
        _ => None,
    }
}
