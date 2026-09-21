//! Per-call metering from Messages `usage` objects, buffered or streamed (v3
//! `shared/claude/{usage,sse}.rs`), and from the compatibility layer's
//! OpenAI shapes.
//!
//! Claude's `input_tokens` already excludes cache reads and cache writes, so
//! it maps straight onto the normalized input; `cache_read_input_tokens` is
//! the cached input and the `cache_creation` breakdown (or the flat
//! `cache_creation_input_tokens`, which is the 5-minute bucket) fills the
//! cache-creation counts. A stream splits the reading in two: `message_start`
//! carries the input side and the final `message_delta` the output side, so
//! the two are merged with the delta's counts preferred where it restates
//! them.

use super::Claudeapi;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame,
    UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use crate::channels::shared::openai_wire;
use gproxy_protocol::WireFamily;
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseFrame};
use gproxy_protocol::connection::StreamFraming;
use rust_decimal::Decimal;
use serde_json::Value;

const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 4 * 1024 * 1024,
    max_value_bytes: 4 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 4 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

fn count(value: &Value, name: &str) -> Option<u64> {
    value.get(name).and_then(Value::as_u64)
}

/// A dimension the API reports either as a bare string or as a named object.
fn string_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_object()?.get("name")?.as_str().map(str::to_owned))
}

/// A Messages `usage` object. Both counts must be present: a reading with
/// only one side is a partial frame, not a result.
pub(super) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    let input = count(usage, "input_tokens")?;
    let output = count(usage, "output_tokens")?;
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    normalized.tokens.input_tokens = Some(input);
    normalized.tokens.output_tokens = Some(output);
    normalized.tokens.cached_input_tokens = count(usage, "cache_read_input_tokens");
    match usage
        .get("cache_creation")
        .filter(|value| value.is_object())
    {
        Some(cache) => {
            normalized.tokens.cache_creation_5m_tokens = count(cache, "ephemeral_5m_input_tokens");
            normalized.tokens.cache_creation_1h_tokens = count(cache, "ephemeral_1h_input_tokens");
        }
        None => {
            normalized.tokens.cache_creation_5m_tokens =
                count(usage, "cache_creation_input_tokens");
        }
    }
    normalized.tokens.reasoning_tokens = usage
        .get("output_tokens_details")
        .and_then(|details| count(details, "thinking_tokens"));
    if let Some(tools) = usage.get("server_tool_use") {
        for (source, target) in [
            ("web_search_requests", "web_searches"),
            ("web_fetch_requests", "web_fetches"),
        ] {
            if let Some(value) = count(tools, source).filter(|value| *value > 0) {
                normalized
                    .metrics
                    .insert(target.into(), Decimal::from(value));
            }
        }
    }
    for name in ["speed", "service_tier", "inference_geo"] {
        if let Some(value) = usage.get(name).and_then(string_value) {
            normalized.dimensions.insert(name.into(), value);
        }
    }
    normalized.actual_service_tier = normalized.dimensions.get("service_tier").cloned();
    Some(normalized)
}

impl UsageExtractor for Claudeapi {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if context.operation.dialect.family() == WireFamily::OpenAi {
            return Ok(openai_wire::extract(&context));
        }
        if !context.response.status.is_success() {
            return Ok(None);
        }
        let Ok(body) = serde_json::from_slice::<Value>(context.response.body) else {
            return Ok(None);
        };
        Ok(body.get("usage").and_then(from_usage))
    }
}

/// `message_start` carries the input side, the final `message_delta` the
/// output side; the merge prefers the delta wherever it restates a count.
fn merge(start: Option<&Value>, delta: Option<&Value>) -> Option<NormalizedUsage> {
    let has = |usage: Option<&Value>, name| usage.is_some_and(|u| count(u, name).is_some());
    if !has(delta, "output_tokens") || !(has(start, "input_tokens") || has(delta, "input_tokens")) {
        return None;
    }
    let mut merged = start
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let delta = delta?.as_object()?;
    for name in [
        "output_tokens",
        "output_tokens_details",
        "server_tool_use",
        "speed",
        "service_tier",
        "inference_geo",
    ] {
        if let Some(value) = delta.get(name) {
            merged.insert(name.into(), value.clone());
        }
    }
    for name in [
        "input_tokens",
        "cache_read_input_tokens",
        "cache_creation",
        "cache_creation_input_tokens",
    ] {
        if delta.contains_key(name) {
            merged.insert(name.into(), delta[name].clone());
        }
    }
    from_usage(&Value::Object(merged))
}

struct MessagesObserver {
    sse: SseDecoder,
    start: Option<Value>,
    delta: Option<Value>,
}

impl MessagesObserver {
    fn see(&mut self, data: &str) {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") if self.start.is_none() => {
                self.start = event.pointer("/message/usage").cloned();
            }
            Some("message_delta")
                if event
                    .pointer("/usage/output_tokens")
                    .is_some_and(Value::is_u64) =>
            {
                self.delta = event.get("usage").cloned();
            }
            _ => {}
        }
    }

    fn usage(&self) -> Option<NormalizedUsage> {
        merge(self.start.as_ref(), self.delta.as_ref())
    }
}

impl UsageObserver for MessagesObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        let UsageFrame::HttpChunk(chunk) = frame else {
            return Ok(());
        };
        let frames = self
            .sse
            .push(chunk)
            .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
        for frame in frames {
            if let SseFrame::Event(event) = frame {
                self.see(&event.data);
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> Option<NormalizedUsage> {
        self.usage()
    }

    fn finish(
        self: Box<Self>,
        _end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        Ok(self.usage())
    }
}

impl UsageStream for Claudeapi {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        if context.operation.dialect.family() == WireFamily::OpenAi {
            return openai_wire::observer(context.operation.operation, context.operation.dialect);
        }
        match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            } => Ok(Box::new(MessagesObserver {
                sse: SseDecoder::new(SSE_LIMITS),
                start: None,
                delta: None,
            })),
            _ => Err(ChannelError::InvalidResponse(
                "Messages streams are SSE".into(),
            )),
        }
    }
}
