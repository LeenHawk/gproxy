use super::super::ResponsesUsageFacts;
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
        self.output = output;
        if let Some(n) = thinking {
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
        _facts: ResponsesUsageFacts,
    ) -> Result<(), TransformError> {
        if self.thinking_at != Some(v.output_tokens) {
            v.output_tokens_details = None;
        }
        Ok(())
    }
}

pub(super) fn claude_tier(tier: Option<r::ServiceTier>) -> Option<Option<c::ResponseServiceTier>> {
    match tier {
        Some(r::ServiceTier::Default) => Some(Some(c::ResponseServiceTier::Standard)),
        Some(r::ServiceTier::Priority) => Some(Some(c::ResponseServiceTier::Priority)),
        _ => None,
    }
}
