mod delta;
mod usage;

use std::collections::BTreeMap;

use gproxy_protocol::claude;

use super::SseFrame;
use crate::TransformError;

use usage::merge_usage;

#[derive(Default)]
pub(super) struct ClaudeCollector {
    message: Option<claude::CreateMessageStartBody>,
    blocks: BTreeMap<u64, claude::ContentBlock>,
    json: BTreeMap<u64, String>,
    delta: Option<claude::MessageDelta>,
    input_transformations: Option<Vec<claude::InputTransformation>>,
    usage: Option<claude::Usage>,
    rest: serde_json::Map<String, serde_json::Value>,
    pub(super) complete: bool,
}

impl ClaudeCollector {
    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<(), TransformError> {
        let event: claude::StreamEvent = serde_json::from_str(&frame.data)?;
        match event {
            claude::StreamEvent::Known(event) => match *event {
                claude::KnownStreamEvent::MessageStart { message, rest } => {
                    self.message = Some(*message);
                    self.rest.extend(rest);
                }
                claude::KnownStreamEvent::ContentBlockStart {
                    index,
                    content_block,
                    rest,
                } => {
                    self.blocks.insert(index, *content_block);
                    self.rest.extend(rest);
                }
                claude::KnownStreamEvent::ContentBlockDelta { index, delta, rest } => {
                    self.apply_delta(index, *delta)?;
                    self.rest.extend(rest);
                }
                claude::KnownStreamEvent::ContentBlockStop { index, rest } => {
                    if let Some(json) = self.json.remove(&index)
                        && let Some(claude::ResponseContentBlock::ToolUse(block)) =
                            self.blocks.get_mut(&index)
                    {
                        block.input = serde_json::from_str(&json)?;
                    }
                    self.rest.extend(rest);
                }
                claude::KnownStreamEvent::MessageDelta {
                    delta,
                    input_transformations,
                    usage,
                    rest,
                    ..
                } => {
                    self.delta = Some(*delta);
                    if input_transformations.is_some() {
                        self.input_transformations = input_transformations;
                    }
                    if let Some(usage) = usage {
                        let current = self.usage.take().or_else(|| {
                            self.message
                                .as_ref()
                                .and_then(|message| message.usage.clone())
                        });
                        self.usage = Some(match current {
                            Some(mut current) => {
                                merge_usage(&mut current, *usage);
                                current
                            }
                            None => *usage,
                        });
                    }
                    self.rest.extend(rest);
                }
                claude::KnownStreamEvent::MessageStop { rest } => {
                    self.complete = true;
                    self.rest.extend(rest);
                }
                claude::KnownStreamEvent::Ping { rest } => self.rest.extend(rest),
                claude::KnownStreamEvent::Error { error, .. } => {
                    return Err(TransformError::unsupported(
                        "Claude stream error",
                        error.message,
                    ));
                }
            },
            claude::StreamEvent::Unknown(object) => {
                self.rest
                    .entry("stream_events")
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()))
                    .as_array_mut()
                    .expect("inserted array")
                    .push(serde_json::to_value(object)?);
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<claude::CreateMessageResponseBody, TransformError> {
        if !self.complete {
            return Err(TransformError::IncompleteStream);
        }
        let message = self
            .message
            .ok_or_else(|| TransformError::shape("Claude stream", "message_start is missing"))?;
        let delta = self.delta.ok_or(TransformError::IncompleteStream)?;
        let stop_reason = delta.stop_reason.ok_or(TransformError::IncompleteStream)?;
        Ok(claude::CreateMessageResponseBody {
            id: message.id,
            type_: message.type_,
            role: message.role,
            content: self.blocks.into_values().collect(),
            model: message.model,
            stop_reason,
            stop_sequence: delta.stop_sequence,
            usage: self.usage.or(message.usage).ok_or_else(|| {
                TransformError::shape("Claude stream", "terminal usage is missing")
            })?,
            container: delta.container,
            context_management: None,
            diagnostics: None,
            input_transformations: self.input_transformations.or(message.input_transformations),
            stop_details: delta.stop_details,
            rest: self.rest,
        })
    }
}
