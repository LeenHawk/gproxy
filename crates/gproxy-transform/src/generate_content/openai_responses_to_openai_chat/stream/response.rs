use gproxy_protocol::openai;

use crate::TransformError;

use super::State;

impl State {
    pub(super) fn response(
        &self,
        status: openai::ResponseStatus,
    ) -> Result<openai::ResponseObject, TransformError> {
        let incomplete_details = match self.finish_reason.as_ref() {
            Some(openai::ChatFinishReason::Length) => {
                Some(crate::wire!(openai::IncompleteDetails {
                    reason: Some(openai::IncompleteReason::MaxOutputTokens),
                    rest: Default::default(),
                }))
            }
            Some(openai::ChatFinishReason::ContentFilter) => {
                Some(crate::wire!(openai::IncompleteDetails {
                    reason: Some(openai::IncompleteReason::ContentFilter),
                    rest: Default::default(),
                }))
            }
            _ => None,
        };
        Ok(crate::wire!(openai::ResponseObject {
            id: self
                .id
                .clone()
                .ok_or_else(|| TransformError::shape("Chat stream", "id missing"))?,
            created_at: self.created_at,
            background: None,
            completed_at: (status == openai::ResponseStatus::Completed)
                .then_some(self.created_at)
                .flatten(),
            conversation: None,
            error: None,
            incomplete_details,
            instructions: None,
            max_output_tokens: None,
            max_tool_calls: None,
            metadata: None,
            model: self.model.clone(),
            moderation: None,
            multi_agent: None,
            object: openai::ResponseObjectType::Response,
            output: self.items.values().cloned().collect(),
            output_text: None,
            parallel_tool_calls: None,
            prompt: None,
            prompt_cache_key: None,
            prompt_cache_options: None,
            prompt_cache_retention: None,
            previous_response_id: None,
            reasoning: None,
            safety_identifier: None,
            service_tier: self.service_tier.clone(),
            status: Some(status),
            store: None,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_logprobs: None,
            top_p: None,
            truncation: None,
            usage: self.usage.clone(),
            user: None,
            rest: Default::default(),
        }))
    }
}
