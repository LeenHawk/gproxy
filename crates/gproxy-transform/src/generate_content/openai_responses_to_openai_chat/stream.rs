use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

mod chat;
mod content;
mod deltas;
mod events;
mod item_events;
mod response;
mod terminal;
mod tool_stream;
mod tools;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}

#[derive(Default)]
pub(crate) struct State {
    id: Option<String>,
    created_at: Option<u64>,
    model: Option<openai::OpenAiModelId>,
    scalar: Option<Item>,
    items: BTreeMap<u32, openai::ResponseItem>,
    started: bool,
    items_finished: bool,
    tools: BTreeMap<u32, Tool>,
    next_index: u32,
    usage: Option<openai::ResponseUsage>,
    finish_reason: Option<openai::ChatFinishReason>,
    sequence: u64,
    service_tier: Option<openai::ServiceTier>,
    stopped: bool,
}

#[derive(Clone)]
struct Item {
    kind: Scalar,
    id: String,
    index: u32,
    text: String,
    logprobs: Vec<openai::TokenLogprob>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar {
    Text,
    Reasoning,
    Refusal,
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
}

impl State {
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
        let events = if frame.data == "[DONE]" {
            self.finish_typed()?
        } else {
            self.push_typed(serde_json::from_str(&frame.data)?)?
        };
        events
            .into_iter()
            .map(|event| SseFrame::typed(event.event_name(), &event))
            .collect()
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        self.finish_typed()?
            .into_iter()
            .map(|event| SseFrame::typed(event.event_name(), &event))
            .collect()
    }
}
