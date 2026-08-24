use gproxy_protocol::openai;

use crate::TransformError;

use super::State;
use super::events::{message_item, reasoning_item, tool_item};

impl State {
    pub(super) fn response(
        &self,
        status: openai::ResponseStatus,
    ) -> Result<openai::ResponseObject, TransformError> {
        let mut indexed = Vec::new();
        if let Some(item) = self.text.as_ref() {
            indexed.push((
                item.index,
                message_item(item, openai::ResponseItemLifecycleStatus::Completed),
            ));
        }
        if let Some(item) = self.reasoning.as_ref() {
            indexed.push((
                item.index,
                reasoning_item(item, openai::ResponseItemLifecycleStatus::Completed),
            ));
        }
        indexed.extend(self.tools.values().map(|item| {
            (
                item.index,
                tool_item(item, openai::ResponseItemLifecycleStatus::Completed),
            )
        }));
        indexed.sort_by_key(|(index, _)| *index);
        let output = indexed.into_iter().map(|(_, item)| item).collect();
        let incomplete_details = match self.finish_reason.as_ref() {
            Some(openai::ChatFinishReason::Length) => Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                rest: Default::default(),
            }),
            Some(openai::ChatFinishReason::ContentFilter) => Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                rest: Default::default(),
            }),
            _ => None,
        };
        Ok(openai::ResponseObject {
            id: self
                .id
                .clone()
                .ok_or_else(|| TransformError::shape("Chat stream", "id missing"))?,
            created_at: self.created_at,
            background: None,
            completed_at: None,
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
            output,
            output_text: self.text.as_ref().map(|item| item.text.clone()),
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
            rest: self.response_rest.clone(),
        })
    }
}
