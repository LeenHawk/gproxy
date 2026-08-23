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
        let claude::StreamEvent::Known(event) = event else {
            return Err(TransformError::unsupported(
                "Claude stream event",
                "unknown event",
            ));
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
                let message = *message;
                if !message.content.is_empty()
                    || message.stop_reason.is_some()
                    || message.stop_sequence.is_some()
                {
                    return Err(TransformError::unsupported(
                        "Claude message_start",
                        "nonempty content or terminal fields",
                    ));
                }
                Some(chunks::metadata(
                    message.id,
                    wire_string(&message.model)?,
                    message
                        .usage
                        .map(super::super::usage::convert)
                        .transpose()?,
                    merge(message.rest, rest),
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
                usage,
                rest,
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
            _ => return Err(TransformError::unsupported("Claude stream", "future event")),
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
        let claude::EventDelta::Known(delta) = delta else {
            return Err(TransformError::unsupported(
                "Claude stream delta",
                "unknown delta",
            ));
        };
        let part = match *delta {
            claude::KnownEventDelta::Text { text, rest: inner } => {
                chunks::text(text, false, merge(inner, rest))
            }
            claude::KnownEventDelta::Thinking {
                thinking,
                rest: inner,
                ..
            } => chunks::text(thinking, true, merge(inner, rest)),
            claude::KnownEventDelta::Signature {
                signature,
                rest: inner,
            } => chunks::signature(signature, merge(inner, rest)),
            claude::KnownEventDelta::InputJson {
                partial_json,
                rest: inner,
            } => {
                let tool = self.tools.get_mut(&index).ok_or_else(|| {
                    TransformError::shape("Claude stream", "tool delta before block start")
                })?;
                tool.partial.push_str(&partial_json);
                tool.block.rest.extend(merge(inner, rest));
                return Ok(None);
            }
            claude::KnownEventDelta::Compaction {
                content,
                rest: inner,
                ..
            } => chunks::text(content, false, merge(inner, rest)),
            other => {
                return Err(TransformError::unsupported(
                    "Claude stream delta",
                    serde_json::to_string(&other)?,
                ));
            }
        };
        Ok(Some(chunks::candidate(
            Some(part),
            None,
            None,
            Default::default(),
        )))
    }
}

fn merge(mut left: gemini::JsonMap, right: gemini::JsonMap) -> gemini::JsonMap {
    left.extend(right);
    left
}
