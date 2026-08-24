use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};
use crate::models::common::wire_string;

use super::claude_block_updates::Block;
use super::state::merge;

#[derive(Clone, Copy)]
pub(crate) enum Output {
    Chat,
    Responses,
}

pub(crate) struct State {
    pub(super) output: Output,
    pub(super) id: Option<String>,
    pub(super) model: Option<openai::OpenAiModelId>,
    pub(super) usage: Option<claude::Usage>,
    pub(super) stop_reason: claude::StopReason,
    pub(super) blocks: BTreeMap<u64, Block>,
    pub(super) completed: Vec<openai::ResponseItem>,
    pub(super) response_rest: openai::Rest,
    pub(super) sequence: u64,
    pub(super) started: bool,
    pub(super) stopped: bool,
}

impl State {
    pub(crate) fn new(output: Output) -> Self {
        Self {
            output,
            id: None,
            model: None,
            usage: None,
            stop_reason: claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
            blocks: BTreeMap::new(),
            completed: Vec::new(),
            response_rest: Default::default(),
            sequence: 0,
            started: false,
            stopped: false,
        }
    }

    fn event(&mut self, event: claude::StreamEvent) -> Result<Vec<Bytes>, TransformError> {
        match event {
            claude::StreamEvent::Known(event) => match *event {
                claude::KnownStreamEvent::MessageStart { message, rest } => {
                    self.start(*message, rest)
                }
                claude::KnownStreamEvent::ContentBlockStart {
                    index,
                    content_block,
                    rest,
                } => self.block_start(index, *content_block, rest),
                claude::KnownStreamEvent::ContentBlockDelta { index, delta, rest } => {
                    self.block_delta(index, *delta, rest)
                }
                claude::KnownStreamEvent::ContentBlockStop { index, rest } => {
                    self.block_stop(index, rest)
                }
                claude::KnownStreamEvent::MessageDelta {
                    delta, usage, rest, ..
                } => self.message_delta(*delta, usage.map(|usage| *usage), rest),
                claude::KnownStreamEvent::MessageStop { rest } => self.message_stop(rest),
                claude::KnownStreamEvent::Ping { rest } => match self.output {
                    Output::Chat => (!rest.is_empty())
                        .then(|| self.chat_chunk(empty_chat_delta(rest), None, None))
                        .transpose()
                        .map(|frame| frame.into_iter().collect()),
                    Output::Responses => {
                        self.response_rest.extend(rest);
                        Ok(Vec::new())
                    }
                },
                claude::KnownStreamEvent::Error { error, .. } => Err(TransformError::unsupported(
                    "Claude stream error",
                    error.message,
                )),
            },
            claude::StreamEvent::Unknown(object) => Err(TransformError::unsupported(
                "Claude stream event",
                serde_json::to_string(&object)?,
            )),
        }
    }

    fn start(
        &mut self,
        message: claude::CreateMessageStartBody,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        if self.started {
            return Err(TransformError::shape(
                "Claude stream",
                "duplicate message_start",
            ));
        }
        self.id = Some(message.id);
        self.model = Some(wire_string(&message.model)?.into());
        self.usage = message.usage;
        self.response_rest = merge(self.response_rest.clone(), message.rest);
        self.response_rest.extend(rest.clone());
        self.started = true;
        Ok(match self.output {
            Output::Chat => {
                let start_rest = std::mem::take(&mut self.response_rest);
                vec![self.chat_chunk(
                    openai::ChatDelta {
                        role: Some(openai::ChatDeltaRole::Assistant),
                        content: Some(String::new()),
                        reasoning_content: None,
                        refusal: None,
                        tool_calls: None,
                        function_call: None,
                        obfuscation: None,
                        rest: start_rest,
                    },
                    None,
                    None,
                )?]
            }
            Output::Responses => vec![self.response_created(
                self.response_object(openai::ResponseStatus::InProgress),
                rest,
            )?],
        })
    }
}

pub(super) fn empty_chat_delta(rest: openai::Rest) -> openai::ChatDelta {
    openai::ChatDelta {
        role: None,
        content: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: None,
        function_call: None,
        obfuscation: None,
        rest,
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
