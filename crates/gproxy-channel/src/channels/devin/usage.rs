//! Per-call metering over the Chat Completions output this channel produces.
//!
//! The counters are the upstream's own, read from the `#7` metadata
//! sub-message of the terminal response frame and carried through
//! `stream.rs`. Their split is not OpenAI's: a paid capture shows
//! `prompt_tokens` (#7.2) excluding both cache reads (#7.5) and cache
//! creation (#7.4), which is exactly `TokenUsage::input_tokens`. The
//! client-facing body follows OpenAI's convention instead, so this reader
//! subtracts the cached detail back out.
//!
//! Cache creation has no TTL on this wire, so it cannot claim one of the
//! `cache_creation_*` fields and travels as a named metric.

use gproxy_protocol::{
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::StreamFraming,
};
use rust_decimal::Decimal;
use serde_json::Value;

use super::Devin;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame,
    UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};

/// Bounds for decoding this channel's own SSE; the host enforces transfer
/// limits of its own.
pub(super) const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 16 * 1024 * 1024,
    max_value_bytes: 16 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 16 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

/// The key a cache-creation count is reported under.
pub const CACHE_CREATION_METRIC: &str = "cache_creation_tokens";

fn from_json(usage: &Value, completeness: UsageCompleteness) -> Option<NormalizedUsage> {
    let prompt = usage.get("prompt_tokens").and_then(Value::as_u64)?;
    let cached = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    let mut normalized = NormalizedUsage {
        completeness,
        ..NormalizedUsage::default()
    };
    normalized.tokens.input_tokens = Some(prompt.saturating_sub(cached.unwrap_or(0)));
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.output_tokens = usage.get("completion_tokens").and_then(Value::as_u64);
    if let Some(created) = usage
        .get("cache_creation_input_tokens")
        .and_then(Value::as_u64)
    {
        normalized
            .metrics
            .insert(CACHE_CREATION_METRIC.into(), Decimal::from(created));
    }
    Some(normalized)
}

impl UsageExtractor for Devin {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !context.response.status.is_success() {
            return Ok(None);
        }
        let Ok(body) = serde_json::from_slice::<Value>(context.response.body) else {
            return Ok(None);
        };
        let Some(usage) = body.get("usage") else {
            return Ok(None);
        };
        Ok(from_json(usage, UsageCompleteness::Complete))
    }
}

struct ChunkObserver {
    sse: SseDecoder,
    usage: Option<Value>,
}

impl ChunkObserver {
    fn see(&mut self, data: &str) {
        if data.trim() == "[DONE]" {
            return;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            return;
        };
        if let Some(usage) = chunk.get("usage")
            && usage.is_object()
        {
            self.usage = Some(usage.clone());
        }
    }

    fn reading(&self, completeness: UsageCompleteness) -> Option<NormalizedUsage> {
        from_json(self.usage.as_ref()?, completeness)
    }
}

impl UsageObserver for ChunkObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        if let UsageFrame::HttpChunk(chunk) = frame {
            let frames = self
                .sse
                .push(chunk)
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            for frame in frames {
                if let SseFrame::Event(event) = frame {
                    self.see(&event.data);
                }
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> Option<NormalizedUsage> {
        self.reading(UsageCompleteness::Partial)
    }

    fn finish(
        self: Box<Self>,
        end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        // The counters arrive whole in the terminal frame, so a stream that
        // carried them is complete even if the host cut the connection after.
        Ok(self.reading(match end {
            UsageStreamEnd::Complete => UsageCompleteness::Complete,
            UsageStreamEnd::Interrupted => UsageCompleteness::Partial,
        }))
    }
}

impl UsageStream for Devin {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            } => Ok(Box::new(ChunkObserver {
                sse: SseDecoder::new(SSE_LIMITS),
                usage: None,
            })),
            _ => Err(ChannelError::InvalidResponse(
                "devin streams are Chat Completions SSE".into(),
            )),
        }
    }
}
