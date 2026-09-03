use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};
use crate::models::common::wire_string;

use super::claude_block_updates::Block;

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
            sequence: 0,
            started: false,
            stopped: false,
        }
    }

    fn event(&mut self, event: claude::StreamEvent) -> Result<Vec<Bytes>, TransformError> {
        match event {
            claude::StreamEvent::Known(event) => match *event {
                claude::KnownStreamEvent::MessageStart { message, .. } => self.start(*message),
                claude::KnownStreamEvent::ContentBlockStart {
                    index,
                    content_block,
                    ..
                } => self.block_start(index, *content_block),
                claude::KnownStreamEvent::ContentBlockDelta { index, delta, .. } => {
                    self.block_delta(index, *delta)
                }
                claude::KnownStreamEvent::ContentBlockStop { index, .. } => self.block_stop(index),
                claude::KnownStreamEvent::MessageDelta { delta, usage, .. } => {
                    self.message_delta(*delta, usage.map(|usage| *usage), Default::default())
                }
                claude::KnownStreamEvent::MessageStop { .. } => {
                    self.message_stop(Default::default())
                }
                claude::KnownStreamEvent::Ping { .. } => Ok(Vec::new()),
                claude::KnownStreamEvent::Error { error, .. } => Err(TransformError::unsupported(
                    "Claude stream error",
                    error.message,
                )),
            },
            claude::StreamEvent::Unknown(_) => Ok(Vec::new()),
        }
    }

    fn start(
        &mut self,
        message: claude::CreateMessageStartBody,
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
        self.started = true;
        Ok(match self.output {
            Output::Chat => {
                vec![self.chat_chunk(
                    openai::ChatDelta {
                        role: Some(openai::ChatDeltaRole::Assistant),
                        content: Some(String::new()),
                        reasoning_content: None,
                        refusal: None,
                        tool_calls: None,
                        function_call: None,
                        obfuscation: None,
                        rest: Default::default(),
                    },
                    None,
                    None,
                )?]
            }
            Output::Responses => vec![
                self.response_created(self.response_object(openai::ResponseStatus::InProgress))?,
            ],
        })
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
