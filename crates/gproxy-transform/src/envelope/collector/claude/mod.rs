mod delta;
mod usage;

use std::collections::{BTreeMap, BTreeSet};

use gproxy_protocol::claude;

use super::SseFrame;
use crate::TransformError;

use usage::merge_usage;

#[derive(Default)]
pub(super) struct ClaudeCollector {
    pub(super) open_tools: BTreeSet<u64>,
    message: Option<claude::CreateMessageStartBody>,
    blocks: BTreeMap<u64, claude::ContentBlock>,
    json: BTreeMap<u64, String>,
    delta: Option<claude::MessageDelta>,
    input_transformations: Option<Vec<claude::InputTransformation>>,
    usage: Option<claude::Usage>,
    pub(super) complete: bool,
}

impl ClaudeCollector {
    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<(), TransformError> {
        let event: claude::StreamEvent = serde_json::from_str(&frame.data)?;
        match event {
            claude::StreamEvent::Known(event) => match *event {
                claude::KnownStreamEvent::MessageStart { message, .. } => {
                    self.message = Some(*message);
                }
                claude::KnownStreamEvent::ContentBlockStart {
                    index,
                    content_block,
                    ..
                } => {
                    if matches!(
                        content_block.as_ref(),
                        claude::ResponseContentBlock::ToolUse(_)
                            | claude::ResponseContentBlock::ServerToolUse(_)
                            | claude::ResponseContentBlock::McpToolUse(_)
                    ) {
                        self.open_tools.insert(index);
                    }
                    if let Some(model) =
                        crate::common::content::claude_fallback_model(&content_block)
                        && let Some(message) = self.message.as_mut()
                    {
                        message.model = model;
                    }
                    self.blocks.insert(index, *content_block);
                }
                claude::KnownStreamEvent::ContentBlockDelta { index, delta, .. } => {
                    self.apply_delta(index, *delta)?;
                }
                claude::KnownStreamEvent::ContentBlockStop { index, .. } => {
                    self.open_tools.remove(&index);
                    if let Some(json) = self.json.remove(&index)
                        && let Some(claude::ResponseContentBlock::ToolUse(block)) =
                            self.blocks.get_mut(&index)
                    {
                        block.input = serde_json::from_str(&json)?;
                    }
                }
                claude::KnownStreamEvent::MessageDelta {
                    delta,
                    input_transformations,
                    usage,
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
                }
                claude::KnownStreamEvent::MessageStop { .. } => {
                    self.complete = true;
                }
                claude::KnownStreamEvent::Ping { .. } => {}
                claude::KnownStreamEvent::Error { error, .. } => {
                    return Err(TransformError::unsupported(
                        "Claude stream error",
                        error.message,
                    ));
                }
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            },
            claude::StreamEvent::Unknown(_) => {}
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
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
        Ok(crate::wire!(claude::CreateMessageResponseBody {
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
            rest: Default::default(),
        }))
    }

    pub(super) fn has_output(&self) -> bool {
        self.blocks.values().any(|block| match block {
            claude::ResponseContentBlock::Text(block) => !block.text.is_empty(),
            claude::ResponseContentBlock::Thinking(block) => !block.thinking.is_empty(),
            claude::ResponseContentBlock::Fallback(_) => false,
            claude::ResponseContentBlock::Raw(raw) => {
                raw.get("type").and_then(serde_json::Value::as_str) != Some("fallback")
            }
            _ => true,
        })
    }
}
