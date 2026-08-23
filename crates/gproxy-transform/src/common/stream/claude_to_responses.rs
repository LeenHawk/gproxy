use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::usage;
use crate::envelope::SseFrame;

use super::claude_to_openai::State;

impl State {
    pub(super) fn response_delta(
        &mut self,
        type_: openai::ResponseStreamEventTypeKnown,
        item_id: String,
        index: u64,
        delta: String,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        let mut event = response_event(type_.clone());
        event.item_id = Some(item_id);
        event.output_index = Some(index as u32);
        if matches!(
            type_,
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextDelta
                | openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDelta
                | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryTextDelta
        ) {
            event.content_index = Some(0);
        }
        event.delta = Some(delta);
        event.sequence_number = Some(self.next_sequence());
        event.rest = rest;
        typed_response_event(event)
    }

    pub(super) fn response_content_part_added(
        &mut self,
        item_id: String,
        output_index: u32,
        part: openai::ResponseContentPart,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        let mut event =
            response_event(openai::ResponseStreamEventTypeKnown::ResponseContentPartAdded);
        event.sequence_number = Some(self.next_sequence());
        event.item_id = Some(item_id);
        event.output_index = Some(output_index);
        event.content_index = Some(0);
        event.part = Some(part);
        event.rest = rest;
        typed_response_event(event)
    }

    pub(super) fn response_event(
        &mut self,
        type_: openai::ResponseStreamEventTypeKnown,
        response: Option<Box<openai::ResponseObject>>,
        item: Option<Box<openai::ResponseItem>>,
        output_index: Option<u32>,
        part: Option<openai::ResponseContentPart>,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        let mut event = response_event(type_);
        event.sequence_number = Some(self.next_sequence());
        event.response = response;
        event.item = item;
        event.output_index = output_index;
        event.part = part;
        event.rest = rest;
        typed_response_event(event)
    }

    pub(super) fn response_object(&self, status: openai::ResponseStatus) -> openai::ResponseObject {
        openai::ResponseObject {
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
            rest: self.response_rest.clone(),
        }
    }

    pub(super) fn next_sequence(&mut self) -> u64 {
        let sequence = self.sequence;
        self.sequence += 1;
        sequence
    }
}

pub(super) fn response_event(
    type_: openai::ResponseStreamEventTypeKnown,
) -> openai::KnownResponseStreamEvent {
    openai::KnownResponseStreamEvent {
        type_,
        sequence_number: None,
        response: None,
        item: None,
        output_index: None,
        content_index: None,
        item_id: None,
        part: None,
        delta: None,
        logprobs: None,
        text: None,
        annotation: None,
        annotation_index: None,
        arguments: None,
        name: None,
        input: None,
        refusal: None,
        summary_index: None,
        partial_image_b64: None,
        partial_image_index: None,
        code: None,
        message: None,
        param: None,
        reasoning_part: None,
        rest: Default::default(),
    }
}

pub(super) fn typed_response_event(
    event: openai::KnownResponseStreamEvent,
) -> Result<Bytes, TransformError> {
    let name = event.type_.as_str().to_owned();
    SseFrame::typed(
        Some(&name),
        &openai::ResponseStreamEvent::Known(Box::new(event)),
    )
}

pub(super) fn reasoning_item(
    id: String,
    text: String,
    signature: Option<String>,
    rest: openai::Rest,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
        id: Some(id),
        summary: Vec::new(),
        content: Some(vec![openai::ResponseReasoningTextPart {
            type_: "reasoning_text".into(),
            text,
            rest: Default::default(),
        }]),
        encrypted_content: signature,
        status: Some(status),
        rest,
    }))
}

pub(super) fn function_item(
    id: String,
    name: String,
    arguments: String,
    rest: openai::Rest,
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
        rest,
    }))
}
