use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};
use crate::generate_content::gemini_generate_content_to_openai_responses::content::ContentConverter;

mod events;
mod parts;
mod response;
mod state;
mod terminal;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::new())
}

pub(crate) struct State {
    id: Option<String>,
    model: Option<openai::OpenAiModelId>,
    pending: Vec<gemini::GenerateContentResponse>,
    content: ContentConverter,
    items: Vec<(u32, openai::ResponseItem)>,
    text: BTreeMap<i32, Item>,
    reasoning: BTreeMap<i32, Item>,
    candidates: Vec<gemini::Candidate>,
    usage: Option<gemini::UsageMetadata>,
    next_index: u32,
    sequence: u64,
    started: bool,
    blocked: bool,
    audio: bool,
    seen_candidates: BTreeSet<i32>,
    finished_candidates: BTreeSet<i32>,
    stopped: bool,
}

#[derive(Clone)]
struct Item {
    id: String,
    index: u32,
    text: String,
    signature: Option<String>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            id: None,
            model: None,
            pending: Vec::new(),
            content: ContentConverter::new(),
            items: Vec::new(),
            text: BTreeMap::new(),
            reasoning: BTreeMap::new(),
            candidates: Vec::new(),
            usage: None,
            next_index: 0,
            sequence: 0,
            started: false,
            blocked: false,
            audio: false,
            seen_candidates: BTreeSet::new(),
            finished_candidates: BTreeSet::new(),
            stopped: false,
        }
    }

    pub(crate) fn push_typed(
        &mut self,
        input: gemini::GenerateContentResponse,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        if self.stopped {
            return Err(TransformError::shape(
                "Gemini stream",
                "chunk received after terminal response",
            ));
        }
        if let Some(id) = input.response_id.clone() {
            self.id = Some(id);
        }
        if let Some(model) = input.model_version.clone() {
            self.model = Some(model.into());
        }
        if !self.started && self.id.is_none() {
            self.pending.push(input);
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        if !self.started {
            output.push(self.created()?);
            self.started = true;
        }
        let mut pending = std::mem::take(&mut self.pending);
        pending.push(input);
        for chunk in pending {
            output.extend(self.chunk(chunk)?);
        }
        Ok(output)
    }

    fn allocate(&mut self) -> u32 {
        let index = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        index
    }

    fn next_sequence(&mut self) -> u64 {
        let sequence = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        sequence
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        encode(self.push_typed(serde_json::from_str(&frame.data)?)?)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        encode(self.finish_typed()?)
    }
}

impl State {
    pub(crate) fn finish_typed(
        &mut self,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        self.terminal()
    }
}

fn encode(events: Vec<openai::ResponseStreamEvent>) -> Result<Vec<Bytes>, TransformError> {
    events
        .into_iter()
        .map(|event| SseFrame::typed(event.event_name(), &event))
        .collect()
}
