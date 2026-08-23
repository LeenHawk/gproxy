use std::collections::BTreeMap;

use gproxy_protocol::claude;

use super::SseFrame;
use crate::TransformError;

#[derive(Default)]
pub(super) struct ClaudeCollector {
    message: Option<claude::CreateMessageStartBody>,
    blocks: BTreeMap<u64, claude::ContentBlock>,
    json: BTreeMap<u64, String>,
    delta: Option<claude::MessageDelta>,
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
                    delta, usage, rest, ..
                } => {
                    self.delta = Some(*delta);
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
                _ => {
                    return Err(TransformError::unsupported(
                        "Claude stream event",
                        "future known event",
                    ));
                }
            },
            claude::StreamEvent::Unknown(raw) => {
                self.rest
                    .entry("stream_events")
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()))
                    .as_array_mut()
                    .expect("inserted array")
                    .push(raw);
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Claude stream",
                    "future event variant",
                ));
            }
        }
        Ok(())
    }

    fn apply_delta(&mut self, index: u64, delta: claude::EventDelta) -> Result<(), TransformError> {
        match delta {
            claude::EventDelta::Known(delta) => match *delta {
                claude::KnownEventDelta::Text { text, .. } => {
                    if let Some(claude::ResponseContentBlock::Text(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.text.push_str(&text);
                    }
                }
                claude::KnownEventDelta::Thinking { thinking, .. } => {
                    if let Some(claude::ResponseContentBlock::Thinking(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.thinking.push_str(&thinking);
                    }
                }
                claude::KnownEventDelta::Signature { signature, .. } => {
                    if let Some(claude::ResponseContentBlock::Thinking(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.signature.get_or_insert_default().push_str(&signature);
                    }
                }
                claude::KnownEventDelta::InputJson { partial_json, .. } => {
                    self.json.entry(index).or_default().push_str(&partial_json);
                }
                claude::KnownEventDelta::Compaction {
                    content,
                    encrypted_content,
                    ..
                } => {
                    if let Some(claude::ResponseContentBlock::Compaction(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.content.get_or_insert_default().push_str(&content);
                        block.encrypted_content.push_str(&encrypted_content);
                    }
                }
                claude::KnownEventDelta::Citations { .. } => {}
                _ => {
                    return Err(TransformError::unsupported(
                        "Claude stream delta",
                        "future known delta",
                    ));
                }
            },
            claude::EventDelta::Unknown(raw) => {
                return Err(TransformError::unsupported(
                    "Claude stream delta",
                    raw.to_string(),
                ));
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Claude stream delta",
                    "future delta variant",
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
            stop_details: delta.stop_details,
            rest: self.rest,
        })
    }
}

fn merge_usage(target: &mut claude::Usage, update: claude::Usage) {
    target.input_tokens = update.input_tokens.or(target.input_tokens);
    target.output_tokens = update.output_tokens.or(target.output_tokens);
    target.cache_creation_input_tokens = update
        .cache_creation_input_tokens
        .or(target.cache_creation_input_tokens);
    target.cache_read_input_tokens = update
        .cache_read_input_tokens
        .or(target.cache_read_input_tokens);
    target.cache_creation = update.cache_creation.or(target.cache_creation.take());
    target.output_tokens_details = update
        .output_tokens_details
        .or(target.output_tokens_details.take());
    target.server_tool_use = update.server_tool_use.or(target.server_tool_use.take());
    target.iterations = update.iterations.or(target.iterations.take());
    target.inference_geo = update.inference_geo.or(target.inference_geo.take());
    target.service_tier = update.service_tier.or(target.service_tier.take());
    target.speed = update.speed.or(target.speed.take());
    target.rest.extend(update.rest);
}
