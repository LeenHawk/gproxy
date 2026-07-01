use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: openai::ResponseStreamEvent,
    ctx: &TransformContext,
) -> Result<claude::StreamEvent, TransformError> {
    let mut transform = StreamTransform;
    let mut output = transform.push(input, ctx)?;
    Ok(output.drain(..).next().unwrap_or_else(ping))
}

#[derive(Default)]
pub struct StreamTransform;

impl StreamTransform {
    pub fn push(
        &mut self,
        input: openai::ResponseStreamEvent,
        _: &TransformContext,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        Ok(match input {
            openai::ResponseStreamEvent::Known(event) => known_event_to_claude(event),
            openai::ResponseStreamEvent::Unknown(_) => Vec::new(),
        })
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        Ok(Vec::new())
    }
}

fn known_event_to_claude(event: openai::KnownResponseStreamEvent) -> Vec<claude::StreamEvent> {
    match event {
        openai::KnownResponseStreamEvent::ResponseCreated { response, .. } => {
            vec![message_start_from_response(*response)]
        }
        openai::KnownResponseStreamEvent::ResponseCompleted { response, .. }
        | openai::KnownResponseStreamEvent::ResponseFailed { response, .. }
        | openai::KnownResponseStreamEvent::ResponseIncomplete { response, .. } => {
            vec![message_delta_from_response(*response)]
        }
        openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
            item, output_index, ..
        } => output_item_added_to_claude(*item, output_index),
        openai::KnownResponseStreamEvent::ResponseOutputItemDone { output_index, .. } => {
            vec![content_block_stop(u64::from(output_index))]
        }
        openai::KnownResponseStreamEvent::ResponseContentPartAdded {
            content_index,
            part,
            ..
        } => vec![content_part_to_claude(content_index, part)],
        openai::KnownResponseStreamEvent::ResponseContentPartDone { .. } => Vec::new(),
        openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
            content_index,
            delta,
            ..
        } => vec![content_delta(u64::from(content_index), text_delta(delta))],
        openai::KnownResponseStreamEvent::ResponseAudioTranscriptDelta { delta, .. } => {
            vec![content_delta(0, text_delta(delta))]
        }
        openai::KnownResponseStreamEvent::ResponseRefusalDelta {
            content_index,
            delta,
            ..
        } => vec![content_delta(u64::from(content_index), text_delta(delta))],
        openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta { delta, .. }
        | openai::KnownResponseStreamEvent::ResponseReasoningTextDelta { delta, .. } => {
            vec![content_delta(0, thinking_delta(delta))]
        }
        openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
            delta,
            output_index,
            ..
        }
        | openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta {
            delta,
            output_index,
            ..
        } => vec![content_delta(
            u64::from(output_index),
            input_json_delta(delta),
        )],
        openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone {
            arguments,
            output_index,
            ..
        } => done_input_to_claude(output_index, arguments),
        openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone {
            input,
            output_index,
            ..
        } => done_input_to_claude(output_index, input),
        openai::KnownResponseStreamEvent::Error { code, message, .. } => {
            vec![known(claude::KnownStreamEvent::Error {
                error: claude::StreamError {
                    type_: code,
                    message,
                    extra: Default::default(),
                },
                extra: Default::default(),
            })]
        }
        _ => Vec::new(),
    }
}

fn message_start_from_response(response: openai::ResponseObject) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::MessageStart {
        message: Box::new(claude::CreateMessageStartBody {
            id: response.id,
            type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
            role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
            content: Vec::new(),
            model: response
                .model
                .map(common::openai_model_string)
                .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned())
                .into(),
            stop_reason: None,
            stop_sequence: None,
            usage: response_usage_to_claude(response.usage),
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}

fn message_delta_from_response(response: openai::ResponseObject) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::MessageDelta {
        context_management: None,
        delta: Box::new(claude::MessageDelta {
            container: None,
            stop_reason: Some(response_stop_reason(&response)),
            stop_sequence: None,
            stop_details: None,
            extra: Default::default(),
        }),
        usage: response
            .usage
            .map(|usage| response_usage_to_claude(Some(usage)))
            .map(Box::new),
        extra: Default::default(),
    })
}

fn output_item_added_to_claude(
    item: openai::ResponseOutputItem,
    output_index: u32,
) -> Vec<claude::StreamEvent> {
    match item.0 {
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            call_id,
            name,
            arguments,
            ..
        })
        | openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            name,
            input: arguments,
            ..
        }) => {
            let mut events = vec![known(claude::KnownStreamEvent::ContentBlockStart {
                index: u64::from(output_index),
                content_block: Box::new(claude::ContentBlock::ToolUse(
                    claude::ResponseToolUseBlock {
                        id: call_id,
                        input: Default::default(),
                        name,
                        type_: claude::ToolUseBlockType::ToolUse,
                        caller: None,
                        extra: Default::default(),
                    },
                )),
                extra: Default::default(),
            })];
            if !arguments.is_empty() {
                events.push(content_delta(
                    u64::from(output_index),
                    input_json_delta(arguments),
                ));
            }
            events
        }
        _ => Vec::new(),
    }
}

fn content_part_to_claude(index: u32, part: openai::ResponseContentPart) -> claude::StreamEvent {
    match part {
        openai::ResponseContentPart::OutputText { text, .. } => {
            content_delta(u64::from(index), text_delta(text))
        }
        openai::ResponseContentPart::Refusal { refusal, .. } => {
            content_delta(u64::from(index), text_delta(refusal))
        }
        openai::ResponseContentPart::ReasoningText { text, .. } => {
            content_delta(u64::from(index), thinking_delta(text))
        }
    }
}

fn response_usage_to_claude(usage: Option<openai::ResponseUsage>) -> claude::Usage {
    common::completion_usage_to_claude(common::response_usage_to_completion(usage))
}

fn response_stop_reason(response: &openai::ResponseObject) -> claude::StopReason {
    if response.output.iter().any(|item| {
        matches!(
            &item.0,
            openai::ResponseItem::Typed(
                openai::TypedResponseItem::FunctionCall { .. }
                    | openai::TypedResponseItem::CustomToolCall { .. }
            )
        )
    }) {
        return claude::StopReason::Known(claude::StopReasonKnown::ToolUse);
    }

    match response.status {
        Some(openai::ResponseStatus::Incomplete) => response
            .incomplete_details
            .as_ref()
            .and_then(|details| details.reason.as_ref())
            .map(incomplete_reason_to_claude)
            .unwrap_or(claude::StopReason::Known(
                claude::StopReasonKnown::MaxTokens,
            )),
        Some(openai::ResponseStatus::Failed | openai::ResponseStatus::Cancelled) => {
            claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        }
        _ => claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
    }
}

fn incomplete_reason_to_claude(reason: &openai::IncompleteReason) -> claude::StopReason {
    match reason {
        openai::IncompleteReason::MaxOutputTokens => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        openai::IncompleteReason::ContentFilter => {
            claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        }
    }
}

fn content_delta(index: u64, delta: claude::KnownEventDelta) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::ContentBlockDelta {
        index,
        delta: Box::new(claude::EventDelta::Known(Box::new(delta))),
        extra: Default::default(),
    })
}

fn content_block_stop(index: u64) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::ContentBlockStop {
        index,
        extra: Default::default(),
    })
}

fn text_delta(text: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::Text {
        text,
        extra: Default::default(),
    }
}

fn thinking_delta(thinking: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::Thinking {
        estimated_tokens: None,
        thinking,
        extra: Default::default(),
    }
}

fn input_json_delta(partial_json: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::InputJson {
        partial_json,
        extra: Default::default(),
    }
}

fn done_input_to_claude(output_index: u32, input: String) -> Vec<claude::StreamEvent> {
    let mut events = Vec::new();
    if !input.is_empty() {
        events.push(content_delta(
            u64::from(output_index),
            input_json_delta(input),
        ));
    }
    events.push(content_block_stop(u64::from(output_index)));
    events
}

fn ping() -> claude::StreamEvent {
    known(claude::KnownStreamEvent::Ping {
        extra: Default::default(),
    })
}

fn known(event: claude::KnownStreamEvent) -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(event))
}
