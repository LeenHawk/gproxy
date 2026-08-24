use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};
use crate::generate_content::openai_responses_to_gemini_generate_content::content::ContentConverter;

use super::config;

mod dispatch;
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
    call_indices: BTreeMap<u32, String>,
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
            call_indices: BTreeMap::new(),
            emitted: BTreeSet::new(),
            text_items: BTreeSet::new(),
            content: ContentConverter::new(),
            stopped: false,
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
