use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_protocol::{claude, gemini};

use crate::TransformError;
use crate::envelope::SseFrame;
use crate::models::common::wire_string;

use super::chunks;

#[derive(Default)]
pub(super) struct State {
    pub(super) tools: BTreeMap<u64, PendingTool>,
    pub(super) pending_signature: Option<String>,
    pub(super) started: bool,
    pub(super) saw_finish: bool,
    pub(super) stopped: bool,
}

pub(super) struct PendingTool {
    pub(super) block: claude::ResponseToolUseBlock,
    pub(super) partial: String,
}

impl State {
    pub(super) fn event(
        &mut self,
        event: claude::StreamEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let event = match event {
            claude::StreamEvent::Known(event) => event,
            claude::StreamEvent::Unknown(_) => return Ok(Vec::new()),
        };
        if self.stopped {
            return Err(TransformError::shape(
                "Claude stream",
                "event after message_stop",
            ));
        }
        if !self.started && !matches!(&*event, claude::KnownStreamEvent::MessageStart { .. }) {
            return Err(TransformError::shape(
                "Claude stream",
                "event before message_start",
            ));
        }
        if self.saw_finish && !matches!(&*event, claude::KnownStreamEvent::MessageStop { .. }) {
            return Err(TransformError::shape(
                "Claude stream",
                "event after terminal stop reason",
            ));
        }
        let chunk = match *event {
            claude::KnownStreamEvent::MessageStart { message, rest } => {
                if self.started {
                    return Err(TransformError::shape(
                        "Claude stream",
                        "duplicate message_start",
                    ));
                }
                self.started = true;
                let mut message = *message;
                if !message.content.is_empty()
                    || message.stop_reason.is_some()
                    || message.stop_sequence.is_some()
                {
                    return Err(TransformError::unsupported(
                        "Claude message_start",
                        "nonempty content or terminal fields",
                    ));
                }
                crate::common::claude_message_controls::preserve_input_transformations(
                    &mut message.rest,
                    message.input_transformations.take(),
                )?;
                Some(chunks::metadata(
                    message.id,
                    wire_string(&message.model)?,
                    message
                        .usage
                        .map(super::super::usage::convert)
                        .transpose()?,
                    chunks::merge(message.rest, rest),
                ))
            }
            claude::KnownStreamEvent::ContentBlockStart {
                index,
                content_block,
                rest,
            } => self.block_start(index, *content_block, rest)?,
            claude::KnownStreamEvent::ContentBlockDelta { index, delta, rest } => {
                self.block_delta(index, *delta, rest)?
            }
            claude::KnownStreamEvent::ContentBlockStop { index, rest } => {
                self.block_stop(index, rest)?
            }
            claude::KnownStreamEvent::MessageDelta {
                context_management,
                delta,
                input_transformations,
                usage,
                mut rest,
            } => {
                if delta.stop_reason.is_some() {
                    if self.saw_finish {
                        return Err(TransformError::shape(
                            "Claude stream",
                            "duplicate terminal stop reason",
                        ));
                    }
                    self.saw_finish = true;
                }
                crate::common::claude_message_controls::preserve_input_transformations(
                    &mut rest,
                    input_transformations,
                )?;
                Some(chunks::message_delta(
                    *delta,
                    context_management,
                    usage.map(|usage| *usage),
                    rest,
                )?)
            }
            claude::KnownStreamEvent::MessageStop { rest } => {
                self.stopped = true;
                (!rest.is_empty()).then(|| chunks::candidate(None, None, None, rest))
            }
            claude::KnownStreamEvent::Ping { rest } => {
                (!rest.is_empty()).then(|| chunks::candidate(None, None, None, rest))
            }
            claude::KnownStreamEvent::Error { error, .. } => {
                return Err(TransformError::unsupported(
                    "Claude stream error",
                    error.message,
                ));
            }
        };
        chunk
            .map(|chunk| SseFrame::typed(None, &chunk).map(|frame| vec![frame]))
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    fn block_delta(
        &mut self,
        index: u64,
        delta: claude::EventDelta,
        rest: gemini::JsonMap,
    ) -> Result<Option<gemini::GenerateContentResponse>, TransformError> {
        let delta = match delta {
            claude::EventDelta::Known(delta) => delta,
            claude::EventDelta::Unknown(_) => return Ok(None),
        };
        let part = match *delta {
            claude::KnownEventDelta::Text { text, rest: inner } => {
                chunks::text(text, false, chunks::merge(inner, rest))
            }
            claude::KnownEventDelta::Thinking {
                thinking,
                rest: inner,
                ..
            } => chunks::text(thinking, true, chunks::merge(inner, rest)),
            claude::KnownEventDelta::Signature {
                signature,
                rest: inner,
            } => {
                self.pending_signature = Some(signature.clone());
                chunks::signature(signature, chunks::merge(inner, rest))
            }
            claude::KnownEventDelta::InputJson {
                partial_json,
                rest: inner,
            } => {
                let tool = self.tools.get_mut(&index).ok_or_else(|| {
                    TransformError::shape("Claude stream", "tool delta before block start")
                })?;
                tool.partial.push_str(&partial_json);
                tool.block.rest.extend(chunks::merge(inner, rest));
                return Ok(None);
            }
            claude::KnownEventDelta::Compaction {
                content,
                rest: inner,
                ..
            } => chunks::text(content, false, chunks::merge(inner, rest)),
            claude::KnownEventDelta::Citations { .. } => return Ok(None),
        };
        Ok(Some(chunks::candidate(
            Some(part),
            None,
            None,
            Default::default(),
        )))
    }
}
