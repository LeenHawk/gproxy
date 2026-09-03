use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use gproxy_protocol::claude;

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

#[derive(Clone, Copy)]
pub(crate) enum Input {
    Chat,
    Responses,
}

pub(crate) struct State {
    pub(super) input: Input,
    pub(super) id: Option<String>,
    pub(super) model: Option<claude::ClaudeModel>,
    pub(super) started: bool,
    pub(super) stopped: bool,
    pub(super) message_delta: bool,
    pub(super) has_tool: bool,
    pub(super) next_index: u64,
    pub(super) scalar: Option<(String, u64)>,
    pub(super) item_indices: BTreeMap<String, u64>,
    pub(super) response_indices: BTreeMap<(String, Option<u32>), u64>,
    pub(super) response_output_indices: BTreeMap<u32, Vec<u64>>,
    pub(super) response_tool_inputs: BTreeMap<u64, String>,
    pub(super) open: BTreeSet<u64>,
}

impl State {
    pub(crate) fn new(input: Input) -> Self {
        Self {
            input,
            id: None,
            model: None,
            started: false,
            stopped: false,
            message_delta: false,
            has_tool: false,
            next_index: 0,
            scalar: None,
            item_indices: BTreeMap::new(),
            response_indices: BTreeMap::new(),
            response_output_indices: BTreeMap::new(),
            response_tool_inputs: BTreeMap::new(),
            open: BTreeSet::new(),
        }
    }

    pub(super) fn ensure_start(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.started {
            return Ok(Vec::new());
        }
        let id = self
            .id
            .clone()
            .ok_or_else(|| TransformError::shape("source stream", "response id is missing"))?;
        let model = self
            .model
            .clone()
            .ok_or_else(|| TransformError::shape("source stream", "model is missing"))?;
        self.started = true;
        let event = claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::MessageStart {
            message: Box::new(claude::CreateMessageStartBody {
                id,
                type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
                role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
                content: Vec::new(),
                model,
                stop_reason: None,
                stop_sequence: None,
                usage: None,
                input_transformations: None,
                rest: Default::default(),
            }),
            rest: Default::default(),
        }));
        Ok(vec![typed_claude("message_start", &event)?])
    }

    pub(super) fn scalar_delta(
        &mut self,
        key: &str,
        kind: Scalar,
        text: String,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        let index = if let Some((open_key, index)) = self.scalar.as_ref() {
            if open_key == key {
                *index
            } else {
                output.extend(self.close(*index)?);
                self.open_scalar(key, kind, &mut output)?
            }
        } else {
            self.open_scalar(key, kind, &mut output)?
        };
        let delta = match kind {
            Scalar::Text => claude::KnownEventDelta::Text {
                text,
                rest: Default::default(),
            },
            Scalar::Thinking => claude::KnownEventDelta::Thinking {
                estimated_tokens: None,
                thinking: text,
                rest: Default::default(),
            },
        };
        output.push(self.delta(index, delta)?);
        Ok(output)
    }

    pub(super) fn response_scalar(
        &mut self,
        item_id: String,
        content_index: Option<u32>,
        delta: String,
        kind: Scalar,
    ) -> Result<Vec<Bytes>, TransformError> {
        let key = (item_id.clone(), content_index);
        let index = if let Some(index) = self
            .response_indices
            .get(&key)
            .or_else(|| self.response_indices.get(&(item_id.clone(), None)))
            .copied()
        {
            index
        } else {
            let index = self.allocate();
            let block = match kind {
                Scalar::Text => claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                    citations: None,
                    text: String::new(),
                    type_: claude::TextBlockType::Text,
                    rest: Default::default(),
                }),
                Scalar::Thinking => claude::ResponseContentBlock::Thinking(claude::ThinkingBlock {
                    signature: None,
                    thinking: String::new(),
                    type_: claude::ThinkingBlockType::Thinking,
                    rest: Default::default(),
                }),
            };
            self.response_indices.insert(key, index);
            let mut output = self.block_start(index, block)?;
            output.extend(self.response_scalar(item_id, content_index, delta, kind)?);
            return Ok(output);
        };
        let delta = match kind {
            Scalar::Text => claude::KnownEventDelta::Text {
                text: delta,
                rest: Default::default(),
            },
            Scalar::Thinking => claude::KnownEventDelta::Thinking {
                estimated_tokens: None,
                thinking: delta,
                rest: Default::default(),
            },
        };
        Ok(vec![self.delta(index, delta)?])
    }

    fn open_scalar(
        &mut self,
        key: &str,
        kind: Scalar,
        output: &mut Vec<Bytes>,
    ) -> Result<u64, TransformError> {
        let index = self.allocate();
        let block = match kind {
            Scalar::Text => claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                citations: None,
                text: String::new(),
                type_: claude::TextBlockType::Text,
                rest: Default::default(),
            }),
            Scalar::Thinking => claude::ResponseContentBlock::Thinking(claude::ThinkingBlock {
                signature: None,
                thinking: String::new(),
                type_: claude::ThinkingBlockType::Thinking,
                rest: Default::default(),
            }),
        };
        output.extend(self.block_start(index, block)?);
        self.scalar = Some((key.into(), index));
        Ok(index)
    }

    pub(super) fn block_start(
        &mut self,
        index: u64,
        block: claude::ContentBlock,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        for open in self.open.clone() {
            output.extend(self.close(open)?);
        }
        self.open.insert(index);
        output.push(typed_claude(
            "content_block_start",
            &claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::ContentBlockStart {
                index,
                content_block: Box::new(block),
                rest: Default::default(),
            })),
        )?);
        Ok(output)
    }

    pub(super) fn delta(
        &self,
        index: u64,
        delta: claude::KnownEventDelta,
    ) -> Result<Bytes, TransformError> {
        if !self.open.contains(&index) {
            return Err(TransformError::shape(
                "source stream",
                "delta targets a closed content block",
            ));
        }
        typed_claude(
            "content_block_delta",
            &claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::ContentBlockDelta {
                index,
                delta: Box::new(claude::EventDelta::Known(Box::new(delta))),
                rest: Default::default(),
            })),
        )
    }

    pub(super) fn input_delta(
        &self,
        index: u64,
        partial_json: String,
    ) -> Result<Bytes, TransformError> {
        self.delta(
            index,
            claude::KnownEventDelta::InputJson {
                partial_json,
                rest: Default::default(),
            },
        )
    }

    pub(super) fn close(&mut self, index: u64) -> Result<Vec<Bytes>, TransformError> {
        if !self.open.remove(&index) {
            return Ok(Vec::new());
        }
        if self
            .scalar
            .as_ref()
            .is_some_and(|(_, value)| *value == index)
        {
            self.scalar = None;
        }
        Ok(vec![typed_claude(
            "content_block_stop",
            &claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::ContentBlockStop {
                index,
                rest: Default::default(),
            })),
        )?])
    }

    pub(super) fn usage_delta(&self, usage: claude::Usage) -> Result<Vec<Bytes>, TransformError> {
        self.message_delta(None, Some(usage))
    }

    pub(super) fn finish_message(
        &mut self,
        reason: claude::StopReason,
        usage: Option<claude::Usage>,
        stop: bool,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        for index in self.open.clone() {
            output.extend(self.close(index)?);
        }
        if !self.message_delta || usage.is_some() {
            output.extend(self.message_delta(Some(reason), usage)?);
            self.message_delta = true;
        }
        if stop {
            output.extend(self.stop()?);
        }
        Ok(output)
    }

    pub(super) fn message_delta(
        &self,
        reason: Option<claude::StopReason>,
        usage: Option<claude::Usage>,
    ) -> Result<Vec<Bytes>, TransformError> {
        let event = claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::MessageDelta {
            context_management: None,
            delta: Box::new(claude::MessageDelta {
                container: None,
                stop_reason: reason,
                stop_sequence: None,
                stop_details: None,
                rest: Default::default(),
            }),
            input_transformations: None,
            usage: usage.map(Box::new),
            rest: Default::default(),
        }));
        Ok(vec![typed_claude("message_delta", &event)?])
    }

    pub(super) fn stop(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        for index in self.open.clone() {
            output.extend(self.close(index)?);
        }
        if !self.message_delta {
            output.extend(self.message_delta(
                Some(claude::StopReason::Known(claude::StopReasonKnown::EndTurn)),
                None,
            )?);
        }
        output.push(typed_claude(
            "message_stop",
            &claude::StreamEvent::Known(Box::new(claude::KnownStreamEvent::MessageStop {
                rest: Default::default(),
            })),
        )?);
        self.stopped = true;
        Ok(output)
    }

    pub(super) fn allocate(&mut self) -> u64 {
        let index = self.next_index;
        self.next_index += 1;
        index
    }

    pub(super) fn response_index(
        &self,
        id: Option<&str>,
        content_index: Option<u32>,
    ) -> Result<u64, TransformError> {
        let id = id
            .ok_or_else(|| TransformError::shape("Responses stream", "delta item_id is missing"))?;
        self.response_indices
            .get(&(id.to_owned(), content_index))
            .or_else(|| self.response_indices.get(&(id.to_owned(), None)))
            .copied()
            .ok_or_else(|| TransformError::shape("Responses stream", "delta before item start"))
    }

    pub(super) fn response_index_for_output(
        &self,
        id: Option<&str>,
        output_index: u32,
        content_index: Option<u32>,
    ) -> Result<u64, TransformError> {
        if id.is_some() {
            return self.response_index(id, content_index);
        }
        let indices = self
            .response_output_indices
            .get(&output_index)
            .ok_or_else(|| TransformError::shape("Responses stream", "delta before item start"))?;
        match indices.as_slice() {
            [index] => Ok(*index),
            _ => Err(TransformError::shape(
                "Responses stream",
                "sparse delta has ambiguous output index",
            )),
        }
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        match self.input {
            Input::Chat => self.chat(frame),
            Input::Responses => self.responses(frame),
        }
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Scalar {
    Text,
    Thinking,
}

fn typed_claude(name: &str, event: &claude::StreamEvent) -> Result<Bytes, TransformError> {
    SseFrame::typed(Some(name), event)
}
