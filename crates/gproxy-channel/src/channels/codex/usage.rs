//! Usage extraction from JSON, HTTP SSE and WebSocket events.

use super::Codex;
use super::common::invalid_response;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame,
    UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseFrame};
use gproxy_protocol::connection::{StreamFraming, WsFrame};
use serde_json::Value;

/// Bounds for watching a Responses SSE stream; the host enforces the real
/// transfer limits, this only keeps the observer's buffers finite.
pub(super) const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 4 * 1024 * 1024,
    max_value_bytes: 4 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 4 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};
/// Responses `usage` -> normalized. Reported input counts include cached
/// tokens; the normalized input excludes them.
fn usage_from_value(usage: &Value) -> Option<NormalizedUsage> {
    let input = usage.get("input_tokens")?.as_u64()?;
    let output = usage.get("output_tokens").and_then(Value::as_u64);
    let cached = usage
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    let reasoning = usage
        .pointer("/output_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64);
    let mut normalized = NormalizedUsage::default();
    normalized.tokens.input_tokens = Some(input.saturating_sub(cached.unwrap_or(0)));
    normalized.tokens.output_tokens = output;
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.reasoning_tokens = reasoning;
    normalized.completeness = UsageCompleteness::Complete;
    Some(normalized)
}

fn usage_from_response_json(body: &[u8]) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/response/usage"))?;
    usage_from_value(usage)
}

impl UsageExtractor for Codex {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        Ok(usage_from_response_json(ctx.response.body))
    }
}

/// Watches a Responses stream for its terminal event's `usage`, over SSE
/// data frames or WebSocket text messages.
struct ResponsesUsageObserver {
    sse: Option<SseDecoder>,
    usage: Option<NormalizedUsage>,
}

impl ResponsesUsageObserver {
    fn see_event(&mut self, data: &str) {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        if matches!(
            value.get("type").and_then(Value::as_str),
            Some("response.completed" | "response.incomplete" | "response.done")
        ) && let Some(usage) = value.pointer("/response/usage").and_then(usage_from_value)
        {
            self.usage = Some(usage);
        }
    }
}

impl UsageObserver for ResponsesUsageObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        match frame {
            UsageFrame::HttpChunk(chunk) => {
                let Some(decoder) = self.sse.as_mut() else {
                    return Ok(());
                };
                let frames = decoder
                    .push(chunk)
                    .map_err(|e| invalid_response(e.to_string()))?;
                for frame in frames {
                    if let SseFrame::Event(event) = frame {
                        self.see_event(&event.data);
                    }
                }
            }
            UsageFrame::WebSocket(WsFrame::Text(text)) => self.see_event(text),
            UsageFrame::WebSocket(_) => {}
        }
        Ok(())
    }
    fn snapshot(&self) -> Option<NormalizedUsage> {
        self.usage.clone()
    }
    fn finish(
        self: Box<Self>,
        _end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        Ok(self.usage)
    }
}

impl UsageStream for Codex {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        let sse = match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            } => Some(SseDecoder::new(SSE_LIMITS)),
            UsageTransport::Http { .. } => {
                return Err(ChannelError::InvalidResponse(
                    "Responses streams are SSE".into(),
                ));
            }
            UsageTransport::WebSocket => None,
        };
        Ok(Box::new(ResponsesUsageObserver { sse, usage: None }))
    }
}
