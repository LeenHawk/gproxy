mod events;

use gproxy_protocol::{claude, openai};

use crate::common::usage;

use super::claude_to_openai::State;

pub(in crate::common::stream) use events::ResponseDelta;

impl State {
    pub(super) fn response_object(&self, status: openai::ResponseStatus) -> openai::ResponseObject {
        crate::wire!(openai::ResponseObject {
            id: self.id.clone().expect("started message has an id"),
            created_at: None,
            background: None,
            completed_at: None,
            conversation: None,
            error: None,
            incomplete_details: match &self.stop_reason {
                claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
                | claude::StopReason::Known(claude::StopReasonKnown::ModelContextWindowExceeded) => {
                    Some(openai::IncompleteDetails {
                        reason: Some(openai::IncompleteReason::MaxOutputTokens),
                        rest: Default::default(),
                    })
                }
                claude::StopReason::Known(claude::StopReasonKnown::Refusal) => {
                    Some(openai::IncompleteDetails {
                        reason: Some(openai::IncompleteReason::ContentFilter),
                        rest: Default::default(),
                    })
                }
                _ => None,
            },
            instructions: None,
            max_output_tokens: None,
            max_tool_calls: None,
            metadata: None,
            model: self.model.clone(),
            moderation: None,
            multi_agent: None,
            object: openai::ResponseObjectType::Response,
            output: self.completed.clone(),
            output_text: None,
            parallel_tool_calls: None,
            prompt: None,
            prompt_cache_key: None,
            prompt_cache_options: None,
            prompt_cache_retention: None,
            previous_response_id: None,
            reasoning: None,
            safety_identifier: None,
            service_tier: None,
            status: Some(status),
            store: None,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_logprobs: None,
            top_p: None,
            truncation: None,
            usage: self.usage.clone().and_then(usage::claude_to_responses),
            user: None,
            rest: Default::default(),
        })
    }

    pub(super) fn next_sequence(&mut self) -> u64 {
        let sequence = self.sequence;
        self.sequence += 1;
        sequence
    }
}

pub(super) fn reasoning_item(
    id: String,
    text: String,
    signature: Option<String>,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
        id: Some(id),
        summary: Vec::new(),
        content: Some(vec![crate::wire!(openai::ResponseReasoningTextPart {
            type_: openai::ResponseReasoningTextType::ReasoningText,
            text,
            rest: Default::default(),
        })]),
        encrypted_content: signature,
        status: Some(status),
        rest: Default::default(),
    }))
}

pub(super) fn function_item(
    id: String,
    name: String,
    arguments: String,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCall {
        arguments,
        call_id: id.clone(),
        name,
        id: Some(id),
        caller: None,
        namespace: None,
        status: Some(status),
        rest: Default::default(),
    }))
}
