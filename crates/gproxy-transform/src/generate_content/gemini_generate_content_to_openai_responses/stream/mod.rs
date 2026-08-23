use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};
use crate::generate_content::openai_responses_to_gemini_generate_content::content::ContentConverter;

use super::config;

mod events;
mod items;
mod terminal;
mod text;
mod tools;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::new())
}

struct State {
    response_id: Option<String>,
    model: Option<String>,
    calls: BTreeMap<String, ToolCall>,
    emitted: BTreeSet<String>,
    text_items: BTreeSet<String>,
    content: ContentConverter,
    stopped: bool,
}

struct ToolCall {
    call_id: String,
    name: String,
    arguments: String,
    custom: bool,
    rest: openai::Rest,
}

impl State {
    fn new() -> Self {
        Self {
            response_id: None,
            model: None,
            calls: BTreeMap::new(),
            emitted: BTreeSet::new(),
            text_items: BTreeSet::new(),
            content: ContentConverter::new(),
            stopped: false,
        }
    }

    fn event(&mut self, event: openai::ResponseStreamEvent) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            return Err(TransformError::shape(
                "Responses stream",
                "event received after terminal event",
            ));
        }
        let openai::ResponseStreamEvent::Known(event) = event else {
            return Err(TransformError::unsupported(
                "Responses stream event",
                "unknown event",
            ));
        };
        if let Some(response) = event.response.as_ref() {
            self.remember(response)?;
        }
        match event.type_ {
            openai::ResponseStreamEventTypeKnown::ResponseCreated
            | openai::ResponseStreamEventTypeKnown::ResponseInProgress
            | openai::ResponseStreamEventTypeKnown::ResponseQueued => Ok(Vec::new()),
            openai::ResponseStreamEventTypeKnown::ResponseOutputTextDelta => {
                self.text_delta(*event, false)
            }
            openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDelta
            | openai::ResponseStreamEventTypeKnown::ResponseReasoningSummaryTextDelta => {
                self.text_delta(*event, true)
            }
            openai::ResponseStreamEventTypeKnown::ResponseRefusalDelta => {
                self.refusal_delta(*event)
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded => {
                self.item_added(*event)
            }
            openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone => self.item_done(*event),
            openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDelta => {
                self.tool_delta(*event, false)
            }
            openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDelta => {
                self.tool_delta(*event, true)
            }
            openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDone => {
                self.tool_done(*event, false)
            }
            openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDone => {
                self.tool_done(*event, true)
            }
            openai::ResponseStreamEventTypeKnown::ResponseCompleted
            | openai::ResponseStreamEventTypeKnown::ResponseIncomplete
            | openai::ResponseStreamEventTypeKnown::ResponseFailed => self.terminal(*event),
            openai::ResponseStreamEventTypeKnown::Error => Err(TransformError::unsupported(
                "Responses stream",
                event.message.as_deref().unwrap_or("error event"),
            )),
            other => events::ignored_or_unsupported(other),
        }
    }

    fn remember(&mut self, response: &openai::ResponseObject) -> Result<(), TransformError> {
        self.response_id = Some(response.id.clone());
        if let Some(model) = response.model.clone() {
            self.model = Some(config::model_string(model)?);
        }
        Ok(())
    }

    fn emit(&self, chunk: gemini::GenerateContentResponse) -> Result<Bytes, TransformError> {
        SseFrame::typed(None, &chunk)
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.event(serde_json::from_str(&frame.data)?)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}
