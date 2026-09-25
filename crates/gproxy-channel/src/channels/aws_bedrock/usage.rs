//! Per-call metering.
//!
//! InvokeModel answers an Anthropic model with the Messages response body, and
//! the streaming translation in `stream.rs` hands the host Messages SSE, so
//! both the buffered extractor and the stream observer read the same
//! `usage` object. Claude's `input_tokens` already excludes cache reads and
//! cache writes, so it maps straight onto the normalized input.
//!
//! This repeats the small Messages mapping that `claudecode` also has rather
//! than sharing it: the two channels are independently feature-gated and a
//! shared module would have to be gated on both. If a third Messages channel
//! arrives it belongs in `channels/shared/`.

use gproxy_protocol::{
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::StreamFraming,
};
use serde_json::Value;

use super::AwsBedrock;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame,
    UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};

/// Bounds for watching the translated stream; the host enforces the real
/// transfer limits, these only keep the observer's buffers finite.
const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: u64::MAX,
    max_value_bytes: u64::MAX,
    max_body_bytes: u64::MAX,
    max_line_bytes: u64::MAX,
    max_part_bytes: 0,
    max_parts: 0,
};

fn count(value: &Value, name: &str) -> Option<u64> {
    value.get(name).and_then(Value::as_u64)
}

/// A Messages `usage` object normalized. Both token counts must be present
/// for the reading to count as usage at all.
pub(super) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    let input = count(usage, "input_tokens")?;
    let output = count(usage, "output_tokens")?;
    let mut normalized = NormalizedUsage::default();
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
    normalized.completeness = UsageCompleteness::Complete;
    Some(normalized)
}

impl UsageExtractor for AwsBedrock {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !context.response.status.is_success() {
            return Ok(None);
        }
        let Ok(body) = serde_json::from_slice::<Value>(context.response.body) else {
            return Ok(None);
        };
        Ok(body.get("usage").and_then(from_usage).map(|mut usage| {
            attach(&mut usage, body.get("model").and_then(Value::as_str).unwrap_or_default(), body["stop_reason"] == "refusal");
            usage
        }))
    }
}

/// `message_start` reports the input side, the final `message_delta` the
/// output side; the merge prefers the delta wherever it reports a count.
fn merge(start: Option<&Value>, delta: Option<&Value>) -> Option<NormalizedUsage> {
    let delta = delta?.as_object()?;
    if !delta.get("output_tokens").is_some_and(Value::is_u64) {
        return None;
    }
    let mut merged = start
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (name, value) in delta {
        merged.insert(name.clone(), value.clone());
    }
    from_usage(&Value::Object(merged))
}

fn attach(usage: &mut NormalizedUsage, model: &str, refused: bool) {
    if model.is_empty() { return; }
    usage.attempts = vec![crate::channel::UsageAttempt {
        model: model.to_owned(),
        usage: Box::new(usage.clone()),
        billable: if refused { usage.tokens.output_tokens.map(|n| n > 0) } else { Some(true) },
        started_at_ms: None,
    }];
}

struct MessagesObserver {
    aws: Option<super::stream::Translator>,
    model: String,
    refused: bool,
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
                self.model = event.pointer("/message/model").and_then(Value::as_str).unwrap_or_default().to_owned();
            }
            Some("message_delta")
                if event
                    .pointer("/usage/output_tokens")
                    .is_some_and(Value::is_u64) =>
            {
                self.delta = event.get("usage").cloned();
                self.refused = event.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("refusal");
            }
            _ => {}
        }
    }

    fn usage(&self) -> Option<NormalizedUsage> {
        merge(self.start.as_ref(), self.delta.as_ref()).map(|mut usage| {
            attach(&mut usage, &self.model, self.refused);
            usage
        })
    }
}

impl UsageObserver for MessagesObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        let UsageFrame::HttpChunk(chunk) = frame else {
            return Ok(());
        };
        let translated;
        let chunk = if let Some(aws) = self.aws.as_mut() {
            translated = aws.push(chunk)?;
            &translated[..]
        } else { chunk };
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

impl UsageStream for AwsBedrock {
    fn accepts_unframed(&self, headers: &http::HeaderMap) -> bool {
        headers.get(http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/vnd.amazon.eventstream"))
    }


    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        let aws = context.headers.get(http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/vnd.amazon.eventstream"));
        if aws || matches!(context.transport, UsageTransport::Http { framing: Some(StreamFraming::Sse) }) {
            Ok(Box::new(MessagesObserver {
                aws: aws.then(super::stream::Translator::new),
                model: String::new(), refused: false,
                sse: SseDecoder::new(SSE_LIMITS), start: None, delta: None,
            }))
        } else {
            Err(ChannelError::InvalidResponse("Bedrock usage requires an event stream".into()))
        }
    }
}
