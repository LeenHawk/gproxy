use gproxy_protocol::openai;

use crate::TransformError;

use super::super::claude_to_openai::{OutputEvent, State};

pub(in crate::common::stream) enum ResponseDelta {
    OutputText,
    ReasoningText,
    FunctionArguments,
}

impl State {
    pub(in crate::common::stream) fn response_delta(
        &mut self,
        kind: ResponseDelta,
        item_id: String,
        index: u64,
        delta: String,
    ) -> Result<OutputEvent, TransformError> {
        let sequence_number = Some(self.next_sequence());
        let output_index = index as u32;
        let event = match kind {
            ResponseDelta::OutputText => openai::KnownResponseStreamEvent::ResponseOutputTextDelta(
                crate::wire!(openai::ResponseOutputTextDeltaEvent {
                    content_index: Some(0),
                    delta,
                    item_id,
                    logprobs: None,
                    output_index,
                    sequence_number,
                    rest: Default::default(),
                }),
            ),
            ResponseDelta::ReasoningText => {
                openai::KnownResponseStreamEvent::ResponseReasoningTextDelta(crate::wire!(
                    openai::ResponseContentDeltaEvent {
                        content_index: 0,
                        delta,
                        item_id,
                        output_index,
                        sequence_number,
                        rest: Default::default(),
                    }
                ))
            }
            ResponseDelta::FunctionArguments => {
                openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta(crate::wire!(
                    openai::ResponseItemStringDeltaEvent {
                        delta,
                        item_id,
                        output_index,
                        sequence_number,
                        rest: Default::default(),
                    }
                ))
            }
        };
        typed_response_event(event)
    }

    pub(in crate::common::stream) fn response_content_part_added(
        &mut self,
        item_id: String,
        output_index: u32,
        part: openai::ResponseContentPart,
    ) -> Result<OutputEvent, TransformError> {
        let sequence_number = Some(self.next_sequence());
        typed_response_event(openai::KnownResponseStreamEvent::ResponseContentPartAdded(
            crate::wire!(openai::ResponseContentPartEvent {
                content_index: 0,
                item_id,
                output_index,
                part,
                sequence_number,
                rest: Default::default(),
            }),
        ))
    }

    pub(in crate::common::stream) fn response_created(
        &mut self,
        response: openai::ResponseObject,
    ) -> Result<OutputEvent, TransformError> {
        let sequence_number = Some(self.next_sequence());
        typed_response_event(openai::KnownResponseStreamEvent::ResponseCreated(
            crate::wire!(openai::ResponseLifecycleEvent {
                response: Box::new(response),
                sequence_number,
                rest: Default::default(),
            }),
        ))
    }

    pub(in crate::common::stream) fn response_terminal(
        &mut self,
        incomplete: bool,
        response: openai::ResponseObject,
    ) -> Result<OutputEvent, TransformError> {
        let payload = crate::wire!(openai::ResponseLifecycleEvent {
            response: Box::new(response),
            sequence_number: Some(self.next_sequence()),
            rest: Default::default(),
        });
        typed_response_event(if incomplete {
            openai::KnownResponseStreamEvent::ResponseIncomplete(payload)
        } else {
            openai::KnownResponseStreamEvent::ResponseCompleted(payload)
        })
    }

    pub(in crate::common::stream) fn response_output_item_added(
        &mut self,
        item: openai::ResponseItem,
        output_index: u32,
    ) -> Result<OutputEvent, TransformError> {
        let sequence_number = Some(self.next_sequence());
        typed_response_event(openai::KnownResponseStreamEvent::ResponseOutputItemAdded(
            crate::wire!(openai::ResponseOutputItemEvent {
                item: Box::new(item),
                output_index,
                sequence_number,
                rest: Default::default(),
            }),
        ))
    }

    pub(in crate::common::stream) fn response_output_item_done(
        &mut self,
        item: openai::ResponseItem,
        output_index: u32,
    ) -> Result<OutputEvent, TransformError> {
        let sequence_number = Some(self.next_sequence());
        typed_response_event(openai::KnownResponseStreamEvent::ResponseOutputItemDone(
            crate::wire!(openai::ResponseOutputItemEvent {
                item: Box::new(item),
                output_index,
                sequence_number,
                rest: Default::default(),
            }),
        ))
    }
}

fn typed_response_event(
    event: openai::KnownResponseStreamEvent,
) -> Result<OutputEvent, TransformError> {
    Ok(OutputEvent::Responses(openai::ResponseStreamEvent::Known(
        Box::new(event),
    )))
}
