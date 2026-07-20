use std::collections::BTreeMap;

use crate::protocol::openai;

use super::output::{OutputState, finish_response};

#[derive(Default)]
pub(super) struct ResponseCollector {
    response: Option<openai::ResponseObject>,
    output: BTreeMap<u32, OutputState>,
    error: Option<openai::ResponseError>,
}

impl ResponseCollector {
    pub(super) fn push(&mut self, event: openai::ResponseStreamEvent) {
        let openai::ResponseStreamEvent::Known(event) = event else {
            return;
        };

        match event {
            openai::KnownResponseStreamEvent::ResponseCreated { response, .. }
            | openai::KnownResponseStreamEvent::ResponseInProgress { response, .. }
            | openai::KnownResponseStreamEvent::ResponseCompleted { response, .. }
            | openai::KnownResponseStreamEvent::ResponseFailed { response, .. }
            | openai::KnownResponseStreamEvent::ResponseIncomplete { response, .. }
            | openai::KnownResponseStreamEvent::ResponseQueued { response, .. } => {
                self.remember_response(*response);
            }
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
                item,
                output_index,
                ..
            } => self.output_state(output_index).seed_item(item.0, false),
            openai::KnownResponseStreamEvent::ResponseOutputItemDone {
                item, output_index, ..
            } => self.output_state(output_index).seed_item(item.0, true),
            openai::KnownResponseStreamEvent::ResponseContentPartAdded {
                content_index,
                item_id,
                output_index,
                part,
                ..
            } => self.output_state(output_index).push_content_part(
                content_index,
                item_id,
                part,
                false,
            ),
            openai::KnownResponseStreamEvent::ResponseContentPartDone {
                content_index,
                item_id,
                output_index,
                part,
                ..
            } => self.output_state(output_index).push_content_part(
                content_index,
                item_id,
                part,
                true,
            ),
            openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
                content_index,
                delta,
                item_id,
                logprobs,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                let part = state.message.text_part(content_index);
                part.push_delta(delta);
                part.push_logprobs(logprobs);
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDone {
                content_index,
                item_id,
                logprobs,
                output_index,
                text,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                let part = state.message.text_part(content_index);
                part.set_done(text);
                part.set_logprobs(logprobs);
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextAnnotationAdded {
                annotation,
                annotation_index,
                content_index,
                item_id,
                output_index,
                ..
            } => {
                if let Ok(annotation) =
                    serde_json::from_value::<openai::ResponseAnnotation>(annotation)
                {
                    let state = self.output_state(output_index);
                    state.message.id.get_or_insert(item_id);
                    state
                        .message
                        .text_part(content_index)
                        .push_annotation(annotation_index, annotation);
                }
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                state.message.refusal_part(content_index).push_delta(delta);
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDone {
                content_index,
                item_id,
                output_index,
                refusal,
                ..
            } => {
                let state = self.output_state(output_index);
                state.message.id.get_or_insert(item_id);
                state.message.refusal_part(content_index).set_done(refusal);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartAdded {
                item_id,
                output_index,
                part,
                summary_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .summary_part(summary_index)
                    .push_delta(part.text);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartDone {
                item_id,
                output_index,
                part,
                summary_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .summary_part(summary_index)
                    .set_done(part.text);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta {
                delta,
                item_id,
                output_index,
                summary_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .summary_part(summary_index)
                    .push_delta(delta);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDone {
                item_id,
                output_index,
                summary_index,
                text,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state.reasoning.summary_part(summary_index).set_done(text);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state
                    .reasoning
                    .content_part(content_index)
                    .push_delta(delta);
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDone {
                content_index,
                item_id,
                output_index,
                text,
                ..
            } => {
                let state = self.output_state(output_index);
                state.reasoning.id.get_or_insert(item_id);
                state.reasoning.content_part(content_index).set_done(text);
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.function_call.item_id.get_or_insert(item_id);
                state.function_call.arguments.push_str(&delta);
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone {
                arguments,
                item_id,
                name,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.function_call.item_id.get_or_insert(item_id);
                state.function_call.name = Some(name);
                state.function_call.done_arguments = Some(arguments);
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.custom_tool_call.item_id.get_or_insert(item_id);
                state.custom_tool_call.input.push_str(&delta);
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone {
                input,
                item_id,
                output_index,
                ..
            } => {
                let state = self.output_state(output_index);
                state.custom_tool_call.item_id.get_or_insert(item_id);
                state.custom_tool_call.done_input = Some(input);
            }
            openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallCodeDelta {
                delta,
                item_id,
                output_index,
                ..
            } => self
                .output_state(output_index)
                .push_code_interpreter_code(item_id, delta, false),
            openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallCodeDone {
                code,
                item_id,
                output_index,
                ..
            } => self
                .output_state(output_index)
                .push_code_interpreter_code(item_id, code, true),
            openai::KnownResponseStreamEvent::ResponseMcpCallArgumentsDelta {
                delta,
                item_id,
                output_index,
                ..
            } => self
                .output_state(output_index)
                .push_mcp_arguments(item_id, delta, false),
            openai::KnownResponseStreamEvent::ResponseMcpCallArgumentsDone {
                arguments,
                item_id,
                output_index,
                ..
            } => self
                .output_state(output_index)
                .push_mcp_arguments(item_id, arguments, true),
            openai::KnownResponseStreamEvent::Error { code, message, .. } => {
                self.error = Some(openai::ResponseError {
                    code: openai::ResponseErrorCode::Unknown(code),
                    message,
                    extra: Default::default(),
                });
            }
            _ => {}
        }
    }

    fn remember_response(&mut self, mut response: openai::ResponseObject) {
        response.extra = Default::default();
        self.response = Some(response);
    }

    fn output_state(&mut self, index: u32) -> &mut OutputState {
        self.output
            .entry(index)
            .or_insert_with(|| OutputState::new(index))
    }

    pub(super) fn finish(self) -> openai::ResponseObject {
        finish_response(self.response, self.output, self.error)
    }
}
