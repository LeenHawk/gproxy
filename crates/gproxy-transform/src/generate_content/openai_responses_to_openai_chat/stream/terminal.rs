use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::events::{emit, message_item, message_part, reasoning_item, tool_item};
use super::{Item, State, Tool, ToolKind};

enum Done {
    Text(Item),
    Reasoning(Item),
    Tool(Tool),
}

impl Done {
    fn index(&self) -> u32 {
        match self {
            Self::Text(item) | Self::Reasoning(item) => item.index,
            Self::Tool(item) => item.index,
        }
    }
}

impl State {
    pub(super) fn stop(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped || !self.started {
            return if self.stopped {
                Ok(Vec::new())
            } else {
                Err(TransformError::IncompleteStream)
            };
        }
        let mut output = Vec::new();
        let mut done = Vec::new();
        done.extend(self.text.clone().map(Done::Text));
        done.extend(self.reasoning.clone().map(Done::Reasoning));
        done.extend(self.tools.values().cloned().map(Done::Tool));
        done.sort_by_key(Done::index);
        for item in done {
            output.extend(self.done(item)?);
        }
        let finish_reason = self.finish_reason.as_ref().ok_or_else(|| {
            TransformError::shape("Chat stream", "DONE before a choice finish_reason")
        })?;
        let status = match finish_reason {
            openai::ChatFinishReason::Length | openai::ChatFinishReason::ContentFilter => {
                openai::ResponseStatus::Incomplete
            }
            openai::ChatFinishReason::Stop
            | openai::ChatFinishReason::ToolCalls
            | openai::ChatFinishReason::FunctionCall => openai::ResponseStatus::Completed,
            openai::ChatFinishReason::Unknown(value) => {
                return Err(TransformError::unsupported("Chat finish reason", value));
            }
        };
        let incomplete = match status {
            openai::ResponseStatus::Incomplete => true,
            openai::ResponseStatus::Completed => false,
            openai::ResponseStatus::Failed
            | openai::ResponseStatus::InProgress
            | openai::ResponseStatus::Cancelled
            | openai::ResponseStatus::Queued => {
                return Err(TransformError::shape(
                    "Responses stream",
                    "unsupported synthesized terminal status",
                ));
            }
            openai::ResponseStatus::Unknown(value) => {
                return Err(TransformError::unsupported("Responses status", value));
            }
        };
        let response = self.response(status)?;
        let payload = openai::ResponseLifecycleEvent {
            response: Box::new(response),
            sequence_number: Some(self.next_sequence()),
            rest: Default::default(),
        };
        output.push(emit(if incomplete {
            openai::KnownResponseStreamEvent::ResponseIncomplete(payload)
        } else {
            openai::KnownResponseStreamEvent::ResponseCompleted(payload)
        })?);
        self.stopped = true;
        Ok(output)
    }

    fn done(&mut self, done: Done) -> Result<Vec<Bytes>, TransformError> {
        match done {
            Done::Text(item) => self.text_done(item),
            Done::Reasoning(item) => self.reasoning_done(item),
            Done::Tool(item) => self.tool_done(item),
        }
    }

    fn text_done(&mut self, item: Item) -> Result<Vec<Bytes>, TransformError> {
        let part = openai::ResponseContentPart::OutputText(message_part(&item));
        Ok(vec![
            emit(openai::KnownResponseStreamEvent::ResponseOutputTextDone(
                openai::ResponseOutputTextDoneEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    logprobs: None,
                    output_index: item.index,
                    sequence_number: Some(self.next_sequence()),
                    text: item.text.clone(),
                    rest: Default::default(),
                },
            ))?,
            emit(openai::KnownResponseStreamEvent::ResponseContentPartDone(
                openai::ResponseContentPartEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    output_index: item.index,
                    part,
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                },
            ))?,
            emit(openai::KnownResponseStreamEvent::ResponseOutputItemDone(
                openai::ResponseOutputItemEvent {
                    item: Box::new(message_item(
                        &item,
                        openai::ResponseItemLifecycleStatus::Completed,
                    )),
                    output_index: item.index,
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                },
            ))?,
        ])
    }

    fn reasoning_done(&mut self, item: Item) -> Result<Vec<Bytes>, TransformError> {
        Ok(vec![
            emit(openai::KnownResponseStreamEvent::ResponseReasoningTextDone(
                openai::ResponseContentTextDoneEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    output_index: item.index,
                    sequence_number: Some(self.next_sequence()),
                    text: item.text.clone(),
                    rest: Default::default(),
                },
            ))?,
            emit(openai::KnownResponseStreamEvent::ResponseOutputItemDone(
                openai::ResponseOutputItemEvent {
                    item: Box::new(reasoning_item(
                        &item,
                        openai::ResponseItemLifecycleStatus::Completed,
                    )),
                    output_index: item.index,
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                },
            ))?,
        ])
    }

    fn tool_done(&mut self, item: Tool) -> Result<Vec<Bytes>, TransformError> {
        let sequence_number = Some(self.next_sequence());
        let done = match item.kind {
            ToolKind::Function => {
                openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone(
                    openai::ResponseFunctionCallArgumentsDoneEvent {
                        arguments: item.arguments.clone(),
                        item_id: Some(item.id.clone()),
                        name: Some(item.name.clone()),
                        output_index: item.index,
                        sequence_number,
                        rest: Default::default(),
                    },
                )
            }
            ToolKind::Custom => openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone(
                openai::ResponseCustomToolCallInputDoneEvent {
                    input: item.arguments.clone(),
                    item_id: item.id.clone(),
                    output_index: item.index,
                    sequence_number,
                    rest: Default::default(),
                },
            ),
        };
        Ok(vec![
            emit(done)?,
            emit(openai::KnownResponseStreamEvent::ResponseOutputItemDone(
                openai::ResponseOutputItemEvent {
                    item: Box::new(tool_item(
                        &item,
                        openai::ResponseItemLifecycleStatus::Completed,
                    )),
                    output_index: item.index,
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                },
            ))?,
        ])
    }
}
