use crate::{
    codec::{self, CodecErrorKind, CodecLimits},
    transform::{Converted, TransformError, TransformErrorKind},
    wire::{
        DeclaredFields,
        claude::{
            generate_content as c,
            stream::{self, StreamEvent},
        },
    },
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub struct ClaudeStreamLimits {
    pub max_events: usize,
    pub max_text_bytes: usize,
    pub max_json_bytes: usize,
    pub max_blocks: usize,
}
impl Default for ClaudeStreamLimits {
    fn default() -> Self {
        Self {
            max_events: 100_000,
            max_text_bytes: 16 * 1024 * 1024,
            max_json_bytes: 16 * 1024 * 1024,
            max_blocks: 100_000,
        }
    }
}
/// Native Claude lifecycle collector. Every failed push poisons the stream;
/// EOF never substitutes for message_stop or an unclosed content block.
pub struct ClaudeStreamCollector {
    message: Option<stream::StreamMessage>,
    events: usize,
    bytes: usize,
    text_bytes: usize,
    limits: ClaudeStreamLimits,
    stopped: bool,
    failed: bool,
    message_delta: bool,
    closed: HashSet<usize>,
    call_ids: HashSet<String>,
    json_buffers: HashMap<usize, String>,
}
impl ClaudeStreamCollector {
    pub fn new(limits: ClaudeStreamLimits) -> Self {
        Self {
            message: None,
            events: 0,
            bytes: 0,
            text_bytes: 0,
            limits,
            stopped: false,
            failed: false,
            message_delta: false,
            closed: HashSet::new(),
            call_ids: HashSet::new(),
            json_buffers: HashMap::new(),
        }
    }
    pub fn push(&mut self, event: StreamEvent) -> Result<(), TransformError> {
        if self.failed {
            return Err(invalid("stream", "collector already failed"));
        }
        let result = self.push_declared(event.into_declared());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_declared(&mut self, event: StreamEvent) -> Result<(), TransformError> {
        if self.stopped {
            return Err(invalid("stream", "event after message_stop"));
        }
        add(&mut self.events, 1, self.limits.max_events, "events")?;
        let encoded = bounded(
            &event,
            self.limits.max_json_bytes.saturating_sub(self.bytes),
        )?;
        add(
            &mut self.bytes,
            encoded.len(),
            self.limits.max_json_bytes,
            "bytes",
        )?;
        match event {
            StreamEvent::MessageStart(event) => {
                if self.message.is_some() {
                    return Err(invalid("message_start", "duplicate start"));
                }
                if event.message.id.is_empty() || event.message.model.is_empty() {
                    return Err(invalid("message_start", "empty message identity or model"));
                }
                if !event.message.content.is_empty()
                    || event.message.stop_reason.flatten().is_some()
                {
                    return Err(invalid(
                        "message_start",
                        "start must precede content and terminal reason",
                    ));
                }

                self.message = Some(event.message);
            }
            StreamEvent::ContentBlockStart(event) => {
                if self.message_delta {
                    return Err(invalid(
                        "content_block_start",
                        "content after message_delta",
                    ));
                }
                let message = self
                    .message
                    .as_mut()
                    .ok_or_else(|| invalid("content_block_start", "missing message_start"))?;
                let index = index(event.index)?;
                if index != message.content.len() {
                    return Err(invalid(
                        "content_block_start.index",
                        "index must append a new block",
                    ));
                }
                if index >= self.limits.max_blocks {
                    return Err(limit("blocks"));
                }
                if let Some(id) = super::blocks::call_id(&event.content_block)
                    && (id.is_empty() || !self.call_ids.insert(id.to_owned()))
                {
                    return Err(invalid(
                        "content_block.id",
                        "empty or duplicate tool call ID",
                    ));
                }
                add(
                    &mut self.text_bytes,
                    super::blocks::text_bytes(&event.content_block),
                    self.limits.max_text_bytes,
                    "text_bytes",
                )?;
                if let c::ResponseContentBlock::Fallback(block) = &event.content_block {
                    if block.from.model != message.model {
                        return Err(invalid(
                            "fallback.from.model",
                            "fallback source differs from current model",
                        ));
                    }
                    if block.to.model.is_empty() {
                        return Err(invalid("fallback.to.model", "empty fallback model"));
                    }
                    message.model.clone_from(&block.to.model);
                }
                message.content.push(event.content_block);
            }
            StreamEvent::ContentBlockDelta(event) => {
                if self.message_delta {
                    return Err(invalid(
                        "content_block_delta",
                        "content after message_delta",
                    ));
                }
                let index = index(event.index)?;
                if self.closed.contains(&index) {
                    return Err(invalid(
                        "content_block_delta.index",
                        "delta after block stop",
                    ));
                }
                let block = self
                    .message
                    .as_mut()
                    .and_then(|m| m.content.get_mut(index))
                    .ok_or_else(|| invalid("content_block_delta.index", "unknown block"))?;
                add(
                    &mut self.text_bytes,
                    super::blocks::delta_text_bytes(&event.delta),
                    self.limits.max_text_bytes,
                    "text_bytes",
                )?;
                super::blocks::apply(
                    block,
                    event.delta,
                    self.json_buffers.entry(index).or_default(),
                )?;
            }
            StreamEvent::ContentBlockStop(event) => {
                let index = index(event.index)?;
                let block = self
                    .message
                    .as_mut()
                    .and_then(|m| m.content.get_mut(index))
                    .ok_or_else(|| invalid("content_block_stop.index", "unknown block"))?;
                if !self.closed.insert(index) {
                    return Err(invalid("content_block_stop.index", "duplicate block stop"));
                }
                if let Some(json) = self.json_buffers.remove(&index).filter(|s| !s.is_empty())
                    && let Some(input) = super::blocks::input(block)
                {
                    *input = serde_json::from_str(&json)
                        .map_err(|e| invalid("tool.input", e.to_string()))?;
                }
            }
            StreamEvent::MessageDelta(event) => {
                let message = self
                    .message
                    .as_mut()
                    .ok_or_else(|| invalid("message_delta", "missing message_start"))?;
                if self.closed.len() != message.content.len() {
                    return Err(invalid("message_delta", "content blocks remain open"));
                }
                self.message_delta = true;
                super::usage::merge(message, *event)?;
            }
            StreamEvent::MessageStop(_) => {
                let message = self
                    .message
                    .as_ref()
                    .ok_or_else(|| invalid("message_stop", "missing message_start"))?;
                if !self.message_delta
                    || self.closed.len() != message.content.len()
                    || message.stop_reason.flatten().is_none()
                {
                    return Err(invalid(
                        "message_stop",
                        "missing message terminal or unclosed content",
                    ));
                }
                self.stopped = true;
            }
            StreamEvent::Error(event) => {
                return Err(invalid(
                    "stream.error",
                    format!("{:?}: {}", event.error.type_, event.error.message),
                ));
            }
            StreamEvent::Ping(_) => {}
        }
        Ok(())
    }
    pub fn finish(self) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
        if self.failed || !self.stopped {
            return Err(invalid("stream", "failed stream or premature EOF"));
        }
        let m = self
            .message
            .ok_or_else(|| invalid("stream", "missing message_start"))?;
        Ok(Converted {
            value: c::GenerateContentResponseBody {
                type_: m.type_,
                id: m.id,
                container: m.container,
                content: m.content,
                context_management: m.context_management,
                diagnostics: m.diagnostics,
                model: m.model,
                role: m.role,
                stop_details: m.stop_details,
                stop_reason: m
                    .stop_reason
                    .flatten()
                    .ok_or_else(|| invalid("stop_reason", "missing terminal reason"))?,
                stop_sequence: m.stop_sequence,
                usage: m.usage,
                rest: Default::default(),
            },
            report: Default::default(),
        })
    }
}
pub(super) fn bounded<T: serde::Serialize>(
    value: &T,
    remaining: usize,
) -> Result<bytes::Bytes, TransformError> {
    let n = remaining as u64;
    codec::encode_json(
        value,
        CodecLimits {
            max_buffer_bytes: n,
            max_value_bytes: n,
            max_body_bytes: n,
            max_line_bytes: n,
            max_part_bytes: n,
            max_parts: 0,
        },
    )
    .map_err(|e| {
        if e.kind() == CodecErrorKind::Limit {
            limit("bytes")
        } else {
            invalid("stream.event", e.to_string())
        }
    })
}
pub(super) fn invalid(field: &str, message: impl Into<String>) -> TransformError {
    TransformError::invalid_result(field, message)
}
fn limit(field: &str) -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        format!("stream.{field}"),
        "Claude stream limit exceeded",
    )
}
fn add(total: &mut usize, n: usize, max: usize, field: &str) -> Result<(), TransformError> {
    *total = total
        .checked_add(n)
        .filter(|n| *n <= max)
        .ok_or_else(|| limit(field))?;
    Ok(())
}
fn index(n: i64) -> Result<usize, TransformError> {
    usize::try_from(n).map_err(|_| invalid("block.index", "negative or unrepresentable index"))
}
