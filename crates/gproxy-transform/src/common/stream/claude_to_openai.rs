use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};
use crate::models::common::wire_string;

use super::claude_block_updates::Block;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Output {
    Chat,
    Responses,
}

#[expect(
    clippy::large_enum_variant,
    reason = "boxing Chat chunks adds a heap allocation to every transformed stream event"
)]
pub(crate) enum OutputEvent {
    Chat(openai::ChatCompletionChunk),
    Responses(openai::ResponseStreamEvent),
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

    pub(crate) fn push_typed(
        &mut self,
        event: claude::StreamEvent,
    ) -> Result<Vec<OutputEvent>, TransformError> {
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
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            },
            claude::StreamEvent::Unknown(_) => Ok(Vec::new()),
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        }
    }

    fn start(
        &mut self,
        message: claude::CreateMessageStartBody,
    ) -> Result<Vec<OutputEvent>, TransformError> {
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
                    crate::wire!(openai::ChatDelta {
                        role: Some(openai::ChatDeltaRole::Assistant),
                        content: Some(String::new()),
                        reasoning_content: None,
                        refusal: None,
                        tool_calls: None,
                        function_call: None,
                        obfuscation: None,
                        rest: Default::default(),
                    }),
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
        let events = self.push_typed(serde_json::from_str(&frame.data)?)?;
        let done = self.stopped && self.output == Output::Chat;
        let mut output = encode(events)?;
        if done {
            output.push(SseFrame::encode(None, "[DONE]"));
        }
        Ok(output)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        encode(self.finish_typed()?)
    }
}

impl State {
    pub(crate) fn finish_typed(&mut self) -> Result<Vec<OutputEvent>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}

fn encode(events: Vec<OutputEvent>) -> Result<Vec<Bytes>, TransformError> {
    events
        .into_iter()
        .map(|event| match event {
            OutputEvent::Chat(event) => SseFrame::typed(None, &event),
            OutputEvent::Responses(event) => SseFrame::typed(event.event_name(), &event),
        })
        .collect()
}
