//! The OpenAI wire mechanics three channels share: the streaming usage opt-in
//! on the request, and per-call metering out of Chat Completions and Responses
//! bodies, buffered or over SSE (v3 `shared/openai/{model,usage,sse}.rs`).
//!
//! `api.openai.com` serves the reference shape, `generativelanguage.googleapis.com`
//! its `/v1beta/openai` compatibility surface and `api.anthropic.com` its
//! `/v1/chat/completions` one; all three report usage the same way, so the
//! reading lives here and each channel only says when to use it.
//!
//! Two usage shapes exist. Chat Completions counts `prompt_tokens` /
//! `completion_tokens` with the cached read under
//! `prompt_tokens_details.cached_tokens`; Responses counts `input_tokens` /
//! `output_tokens` with `input_tokens_details.cached_tokens`. Both prompt
//! totals *include* the cached read and, where an OpenAI-compatible vendor
//! reports one, the cache write named `cache_write_tokens`. `input_tokens` is
//! the ordinary input alone, so both come back out of it and are carried in
//! the fields that name them — the same reading `codex` and
//! `shared::vendor_usage` take of the identical wire.

use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageFrame, UsageObserver,
    UsageStreamEnd,
};
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseFrame};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, Operation};
use rust_decimal::Decimal;
use serde_json::Value;

/// Bounds for watching a stream; the host enforces the real transfer limits,
/// this only keeps the observer's own buffers finite.
const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 4 * 1024 * 1024,
    max_value_bytes: 4 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 4 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

// ------------------------------------------------------------------ request

/// Ask a Chat Completions stream to end with a usage chunk
/// (`stream_options.include_usage`). Responses streams report usage on
/// `response.completed` without being asked, so this is Chat-only. A body that
/// is not a JSON object, or whose `stream_options` is not an object, is left
/// exactly as the client sent it: metering never breaks a request.
pub(crate) fn stream_usage_opt_in(bytes: Bytes) -> Bytes {
    let Some(mut body) = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
    else {
        return bytes;
    };
    let Some(root) = body.as_object_mut() else {
        return bytes;
    };
    let options = root
        .entry("stream_options")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Some(options) = options.as_object_mut() else {
        return bytes;
    };
    options.insert("include_usage".into(), Value::Bool(true));
    Bytes::from(body.to_string())
}

// ------------------------------------------------------------------ reading

fn count(value: &Value, name: &str) -> Option<u64> {
    value.get(name).and_then(Value::as_u64)
}

fn present(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(Value::is_u64)
}

fn positive(value: &Value, name: &str) -> u64 {
    count(value, name).unwrap_or_default()
}

fn decimal(value: Option<&Value>) -> Option<Decimal> {
    match value? {
        Value::Number(number) => number.to_string().parse().ok(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn metric(usage: &mut NormalizedUsage, name: &str, value: u64) {
    if value > 0 {
        usage.metrics.insert(name.into(), Decimal::from(value));
    }
}

/// The serving tier the upstream actually used, either at the top level of a
/// buffered reply or under `response` in a Responses envelope. It may be a
/// string or an object with a `type`.
fn service_tier(value: &Value) -> Option<String> {
    let tier = value
        .get("service_tier")
        .or_else(|| value.pointer("/response/service_tier"))?;
    tier.as_str()
        .or_else(|| tier.get("type")?.as_str())
        .map(str::to_owned)
}

fn apply_service_tier(usage: &mut NormalizedUsage, tier: Option<String>) {
    if let Some(tier) = tier {
        usage.dimensions.insert("service_tier".into(), tier.clone());
        usage.actual_service_tier = Some(tier);
    }
}

/// A `usage` object in either shape. Chat is recognized first because a
/// Responses object never carries `prompt_tokens`.
pub(crate) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    let chat = present(usage, "prompt_tokens")
        && (present(usage, "completion_tokens") || present(usage, "total_tokens"));
    let (prompt, completion, input_details, output_details) = if chat {
        (
            "prompt_tokens",
            "completion_tokens",
            "prompt_tokens_details",
            "completion_tokens_details",
        )
    } else if present(usage, "input_tokens") && present(usage, "output_tokens") {
        (
            "input_tokens",
            "output_tokens",
            "input_tokens_details",
            "output_tokens_details",
        )
    } else {
        return None;
    };
    let empty = Value::Null;
    let input = usage.get(input_details).unwrap_or(&empty);
    let output = usage.get(output_details).unwrap_or(&empty);
    // OpenAI's reported prompt total counts the cache read and, where a
    // compatible vendor reports one, the cache write. `input_tokens` is the
    // ordinary input alone, so both come back out of it and are carried in
    // the fields that name them.
    let cached = count(input, "cached_tokens");
    let cache_write = positive(input, "cache_write_tokens");
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    normalized.tokens.input_tokens = Some(
        positive(usage, prompt)
            .saturating_sub(cached.unwrap_or_default())
            .saturating_sub(cache_write),
    );
    normalized.tokens.output_tokens = Some(positive(usage, completion));
    normalized.tokens.cached_input_tokens = cached;
    if cache_write > 0 {
        normalized.tokens.cache_creation_30m_tokens = Some(cache_write);
    }
    normalized.tokens.reasoning_tokens = count(output, "reasoning_tokens");
    metric(
        &mut normalized,
        "audio_input_tokens",
        positive(input, "audio_tokens"),
    );
    metric(
        &mut normalized,
        "audio_output_tokens",
        positive(output, "audio_tokens"),
    );
    let searches = usage
        .get("server_tool_use_details")
        .or_else(|| usage.get("server_tool_use"))
        .map(|tools| positive(tools, "web_search_requests"))
        .unwrap_or_default();
    metric(&mut normalized, "web_searches", searches);
    Some(normalized)
}

/// An image reply: the produced image tokens leave `output_tokens` and become
/// `image_output_tokens`, so text and image output are priced apart.
fn from_image(value: &Value) -> Option<NormalizedUsage> {
    let usage = value.get("usage")?;
    let mut normalized = from_usage(usage)?;
    let details = usage
        .get("completion_tokens_details")
        .or_else(|| usage.get("output_tokens_details"));
    let image = details
        .and_then(|details| count(details, "image_tokens"))
        .or(normalized.tokens.output_tokens)
        .unwrap_or_default();
    normalized.tokens.output_tokens = normalized
        .tokens
        .output_tokens
        .map(|total| total.saturating_sub(image));
    metric(&mut normalized, "image_output_tokens", image);
    if let Some(outputs) = value.get("data").and_then(Value::as_array) {
        normalized
            .metrics
            .insert("image_outputs".into(), Decimal::from(outputs.len()));
    }
    Some(normalized)
}

/// A transcription reply, which reports either tokens or `seconds` of audio
/// depending on the model. `"type": "duration"` marks the seconds-only shape.
fn from_transcription(value: &Value) -> Option<NormalizedUsage> {
    let usage = value.get("usage")?;
    let tokens = usage.get("type").and_then(Value::as_str) != Some("duration")
        && present(usage, "input_tokens")
        && present(usage, "output_tokens");
    let seconds = decimal(usage.get("seconds"));
    if !tokens && seconds.is_none() {
        return None;
    }
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    if tokens {
        normalized.tokens.input_tokens = Some(positive(usage, "input_tokens"));
        normalized.tokens.output_tokens = Some(positive(usage, "output_tokens"));
    }
    if let Some(seconds) = seconds {
        normalized.metrics.insert("audio_seconds".into(), seconds);
    }
    Some(normalized)
}

/// Usage from a buffered reply. `None` means the reply reports none, which is
/// not the same as zero consumption.
pub(crate) fn extract(context: &UsageContext<'_>) -> Option<NormalizedUsage> {
    if !context.response.status.is_success() {
        return None;
    }
    let value = serde_json::from_slice::<Value>(context.response.body).ok()?;
    let mut usage = match context.operation.operation {
        Operation::CreateImage | Operation::EditImage => from_image(&value)?,
        Operation::CreateTranscription => from_transcription(&value)?,
        _ => from_usage(value.get("usage")?)?,
    };
    apply_service_tier(&mut usage, service_tier(&value));
    Some(usage)
}

// ----------------------------------------------------------------- streaming

/// Which records of a stream carry the usage this channel is watching for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Chat Completions: any chunk may carry `usage`; with
    /// `stream_options.include_usage` the last one does.
    Chat,
    /// Responses: `response.completed` carries `response.usage`.
    Responses,
    Image,
    Transcription,
}

/// The stream shape for an operation and its native dialect, or `None` when
/// the channel reports no streamed usage for it.
fn kind(operation: Operation, dialect: Dialect) -> Option<Kind> {
    match operation {
        Operation::StreamGenerateContent | Operation::GenerateContent => match dialect {
            Dialect::OpenAiChat => Some(Kind::Chat),
            Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => Some(Kind::Responses),
            _ => None,
        },
        Operation::CreateImage | Operation::EditImage => Some(Kind::Image),
        Operation::CreateTranscription => Some(Kind::Transcription),
        _ => None,
    }
}

struct Observer {
    kind: Kind,
    sse: SseDecoder,
    usage: Option<NormalizedUsage>,
    tier: Option<String>,
}

impl Observer {
    /// The last record that reports usage wins; the shapes above are
    /// cumulative, never deltas, so nothing is summed across records.
    fn see(&mut self, event: Option<&str>, data: &str) {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        if let Some(tier) = service_tier(&value) {
            self.tier = Some(tier);
        }
        let completed = |value: &Value| {
            event == Some("response.completed")
                || value.get("type").and_then(Value::as_str) == Some("response.completed")
        };
        let usage = match self.kind {
            Kind::Chat => value.get("usage").and_then(from_usage),
            Kind::Responses if completed(&value) => {
                value.pointer("/response/usage").and_then(from_usage)
            }
            Kind::Responses => None,
            Kind::Image => from_image(&value),
            Kind::Transcription
                if value.get("type").and_then(Value::as_str) == Some("transcript.text.done") =>
            {
                from_transcription(&value)
            }
            Kind::Transcription => None,
        };
        if let Some(mut usage) = usage {
            apply_service_tier(&mut usage, self.tier.clone());
            self.usage = Some(usage);
        }
    }
}

impl UsageObserver for Observer {
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
                self.see(event.event.as_deref(), &event.data);
            }
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

/// An observer for an SSE stream of `operation` in `dialect`, or an error when
/// the channel has nothing to watch for there.
pub(crate) fn observer(
    operation: Operation,
    dialect: Dialect,
) -> Result<Box<dyn UsageObserver>, ChannelError> {
    let kind = kind(operation, dialect).ok_or_else(|| {
        ChannelError::InvalidResponse("operation reports no streamed OpenAI usage".into())
    })?;
    Ok(Box::new(Observer {
        kind,
        sse: SseDecoder::new(SSE_LIMITS),
        usage: None,
        tier: None,
    }))
}
