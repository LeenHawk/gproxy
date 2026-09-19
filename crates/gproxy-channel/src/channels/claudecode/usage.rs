//! Per-call metering from Messages `usage` objects, buffered or streamed
//! (v3 `shared/claude/{usage,sse}.rs`). Claude's `input_tokens` already
//! excludes cache reads and cache writes, so it maps straight onto the
//! normalized input; `cache_read_input_tokens` is the cached input and the
//! `cache_creation` breakdown (or the flat `cache_creation_input_tokens`,
//! which is 5-minute) fills the cache-creation counts. When a response
//! carries server-side fallback `iterations`, each becomes a billing attempt.

use super::{Claudecode, invalid_response};
use crate::channel::{
    ChannelError, NormalizedUsage, UsageAttempt, UsageCompleteness, UsageContext, UsageExtractor,
    UsageFrame, UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use gproxy_protocol::{
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::StreamFraming,
};
use rust_decimal::Decimal;
use serde_json::Value;

/// Bounds for watching a Messages SSE stream; the host enforces the real
/// transfer limits, this only keeps the observer's buffers finite.
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

fn string_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_object()?.get("name")?.as_str().map(str::to_owned))
}

/// A `usage` object -> normalized; both `input_tokens` and `output_tokens`
/// must be present for the reading to count.
pub(super) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    let input = count(usage, "input_tokens")?;
    let output = count(usage, "output_tokens")?;
    let mut normalized = NormalizedUsage::default();
    normalized.tokens.input_tokens = Some(input);
    normalized.tokens.output_tokens = Some(output);
    normalized.tokens.cached_input_tokens = count(usage, "cache_read_input_tokens");
    match usage.get("cache_creation").filter(|v| v.is_object()) {
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
            if let Some(value) = count(tools, source).filter(|v| *v > 0) {
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
    normalized.completeness = UsageCompleteness::Complete;
    Some(normalized)
}

/// Server-side fallback `iterations` become per-attempt usage; an attempt
/// that produced output is billable, as is the final one unless refused.
fn attach_attempts(usage: &mut NormalizedUsage, wire: &Value, model: &str, refused: bool) {
    let Some(iterations) = wire
        .get("iterations")
        .and_then(Value::as_array)
        .filter(|items| items.iter().any(|item| item["type"] == "fallback_message"))
    else {
        return;
    };
    for (index, item) in iterations.iter().enumerate() {
        let Some(normalized) = from_usage(item) else {
            continue;
        };
        let produced = normalized.tokens.output_tokens.is_some_and(|n| n > 0);
        usage.attempts.push(UsageAttempt {
            model: item
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(model)
                .into(),
            billable: Some(produced || (!refused && index + 1 == iterations.len())),
            usage: Box::new(normalized),
            started_at_ms: None,
        });
    }
}

impl UsageExtractor for Claudecode {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        let Ok(body) = serde_json::from_slice::<Value>(ctx.response.body) else {
            return Ok(None);
        };
        let Some(wire) = body.get("usage") else {
            return Ok(None);
        };
        let Some(mut usage) = from_usage(wire) else {
            return Ok(None);
        };
        attach_attempts(
            &mut usage,
            wire,
            body.get("model")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            body["stop_reason"] == "refusal",
        );
        Ok(Some(usage))
    }
}

/// `message_start` carries the input side, the final `message_delta` the
/// output side (and, after a fallback, the input side again); the merge
/// prefers the delta's counts where it reports them.
fn merge_stream(start: Option<&Value>, delta: Option<&Value>) -> Option<NormalizedUsage> {
    let start_has_input = start.is_some_and(|u| count(u, "input_tokens").is_some());
    let delta_has_input = delta.is_some_and(|u| count(u, "input_tokens").is_some());
    let delta_has_output = delta.is_some_and(|u| count(u, "output_tokens").is_some());
    if !delta_has_output || (!start_has_input && !delta_has_input) {
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
        "iterations",
    ] {
        if let Some(value) = delta.get(name) {
            merged.insert(name.into(), value.clone());
        }
    }
    for name in ["input_tokens", "cache_read_input_tokens"] {
        if delta.get(name).is_some_and(Value::is_u64) {
            merged.insert(name.into(), delta[name].clone());
        }
    }
    let start_has_creation = start.is_some_and(has_cache_creation);
    let delta_value = Value::Object(delta.clone());
    let delta_has_breakdown = delta.get("cache_creation").is_some_and(Value::is_object);
    if delta_has_breakdown || (!start_has_creation && has_cache_creation(&delta_value)) {
        for name in ["cache_creation", "cache_creation_input_tokens"] {
            if let Some(value) = delta.get(name) {
                merged.insert(name.into(), value.clone());
            }
        }
    }
    from_usage(&Value::Object(merged))
}

fn has_cache_creation(usage: &Value) -> bool {
    usage.get("cache_creation").is_some_and(Value::is_object)
        || count(usage, "cache_creation_input_tokens").is_some()
}

/// Watches a Messages SSE stream for `message_start`, fallback
/// `content_block_start` and the terminal `message_delta`.
struct MessagesUsageObserver {
    sse: SseDecoder,
    start: Option<Value>,
    delta: Option<Value>,
    model: String,
    refused: bool,
}

impl MessagesUsageObserver {
    fn see_event(&mut self, data: &str) {
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") if self.start.is_none() => {
                self.start = event.pointer("/message/usage").cloned();
                self.model = event
                    .pointer("/message/model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into();
            }
            Some("content_block_start")
                if event.pointer("/content_block/type").and_then(Value::as_str)
                    == Some("fallback") =>
            {
                if let Some(model) = event
                    .pointer("/content_block/to/model")
                    .and_then(Value::as_str)
                {
                    self.model = model.into();
                }
            }
            Some("message_delta")
                if event
                    .pointer("/usage/output_tokens")
                    .is_some_and(Value::is_u64) =>
            {
                self.delta = event.get("usage").cloned();
                self.refused =
                    event.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("refusal");
            }
            _ => {}
        }
    }

    fn usage(&self) -> Option<NormalizedUsage> {
        let mut usage = merge_stream(self.start.as_ref(), self.delta.as_ref())?;
        attach_attempts(
            &mut usage,
            self.delta.as_ref().unwrap_or(&Value::Null),
            &self.model,
            self.refused,
        );
        Some(usage)
    }
}

impl UsageObserver for MessagesUsageObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        let UsageFrame::HttpChunk(chunk) = frame else {
            return Ok(());
        };
        let frames = self
            .sse
            .push(chunk)
            .map_err(|e| invalid_response(e.to_string()))?;
        for frame in frames {
            if let SseFrame::Event(event) = frame {
                self.see_event(&event.data);
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

impl UsageStream for Claudecode {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            } => Ok(Box::new(MessagesUsageObserver {
                sse: SseDecoder::new(SSE_LIMITS),
                start: None,
                delta: None,
                model: String::new(),
                refused: false,
            })),
            _ => Err(ChannelError::InvalidResponse(
                "Messages streams are SSE".into(),
            )),
        }
    }
}
