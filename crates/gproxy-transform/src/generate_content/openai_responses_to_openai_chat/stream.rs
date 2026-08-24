use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

mod chat;
mod deltas;
mod events;
mod response;
mod terminal;
mod tool_stream;
mod tools;

use events::emit;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}

#[derive(Default)]
struct State {
    id: Option<String>,
    created_at: Option<u64>,
    model: Option<openai::OpenAiModelId>,
    text: Option<Item>,
    reasoning: Option<Item>,
    tools: BTreeMap<u32, Tool>,
    next_index: u32,
    usage: Option<openai::ResponseUsage>,
    finish_reason: Option<openai::ChatFinishReason>,
    sequence: u64,
    service_tier: Option<openai::ServiceTier>,
    response_rest: openai::Rest,
    started: bool,
    stopped: bool,
}

#[derive(Clone)]
struct Item {
    id: String,
    index: u32,
    text: String,
    rest: openai::Rest,
    logprobs: Vec<openai::TokenLogprob>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Function,
    Custom,
}

#[derive(Clone)]
struct Tool {
    id: String,
    index: u32,
    name: String,
    arguments: String,
    kind: ToolKind,
    rest: openai::Rest,
}

impl State {
    fn ensure_start(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.started {
            return Ok(Vec::new());
        }
        let response = self.response(openai::ResponseStatus::InProgress)?;
        self.started = true;
        let sequence_number = Some(self.next_sequence());
        Ok(vec![emit(
            openai::KnownResponseStreamEvent::ResponseCreated(openai::ResponseLifecycleEvent {
                response: Box::new(response),
                sequence_number,
                rest: Default::default(),
            }),
        )?])
    }

    fn item_id(&self, prefix: &str) -> Result<String, TransformError> {
        self.id
            .as_ref()
            .map(|id| format!("{prefix}_{id}"))
            .ok_or_else(|| TransformError::shape("Chat stream", "id missing"))
    }

    fn allocate(&mut self) -> u32 {
        let value = self.next_index;
        self.next_index += 1;
        value
    }

    fn next_sequence(&mut self) -> u64 {
        let value = self.sequence;
        self.sequence += 1;
        value
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.chat(frame)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}
