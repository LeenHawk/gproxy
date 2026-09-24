//! Per-call metering for AI Studio, buffered and streamed.
//!
//! The native surface reports `usageMetadata` (v3 `shared/gemini/usage.rs`):
//! `promptTokenCount` is the whole input including the cached read, which
//! `cachedContentTokenCount` names separately, so the cached part comes back
//! out of the normalized input and rides in the field that names it.
//! `candidatesTokenCount` counts every produced token; the image and audio
//! parts of it are broken out of `candidatesTokensDetails` so they can be
//! priced apart, and `thoughtsTokenCount` is added because Google reports
//! thinking outside the candidate count while the normalized output includes
//! it. The compatibility surface reports OpenAI's shapes, read by
//! `channels::shared::openai_wire`.
//!
//! Streaming carries the same object on the records of a
//! `streamGenerateContent` response, framed either as SSE or as one
//! incrementally delivered JSON array. Later records restate the running
//! totals rather than adding to them, so the last complete reading wins.

use super::Aistudio;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame,
    UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use crate::channels::shared::openai_wire;
use gproxy_protocol::codec::{CodecLimits, JsonArrayDecoder, SseDecoder, SseFrame};
use gproxy_protocol::connection::StreamFraming;
use gproxy_protocol::{Operation, WireFamily};
use rust_decimal::Decimal;
use serde_json::Value;

const LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: u64::MAX,
    max_value_bytes: u64::MAX,
    max_body_bytes: u64::MAX,
    max_line_bytes: u64::MAX,
    max_part_bytes: 0,
    max_parts: 0,
};

/// The response header AI Studio uses to name the tier it actually served.
const SERVICE_TIER: &str = "x-gemini-service-tier";

fn count(value: &Value, name: &str) -> Option<u64> {
    value.get(name).and_then(Value::as_u64)
}

/// Tokens of one modality inside a `*TokensDetails` array.
fn modality(details: Option<&Value>, name: &str) -> u64 {
    details
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("modality").and_then(Value::as_str) == Some(name))
        .filter_map(|entry| count(entry, "tokenCount"))
        .sum()
}

fn metric(usage: &mut NormalizedUsage, name: &str, value: u64) {
    if value > 0 {
        usage.metrics.insert(name.into(), Decimal::from(value));
    }
}

/// A `usageMetadata` object. Both required counts must be present: a record
/// that only carries `promptTokenCount` is an early partial reading, not a
/// result, and reporting it would understate the call.
pub(super) fn from_metadata(metadata: &Value) -> Option<NormalizedUsage> {
    let prompt = count(metadata, "promptTokenCount")?;
    let candidates = count(metadata, "candidatesTokenCount")?;
    let cached = count(metadata, "cachedContentTokenCount")
        .unwrap_or_default()
        .min(prompt);
    let thoughts = count(metadata, "thoughtsTokenCount").unwrap_or_default();
    let details = metadata.get("candidatesTokensDetails");
    let image = modality(details, "IMAGE");
    let audio = modality(details, "AUDIO");
    let mut usage = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    // `promptTokenCount` is the whole input including the cached read, which
    // `input_tokens` excludes.
    usage.tokens.input_tokens = Some(prompt - cached);
    // Google counts thinking outside `candidatesTokenCount`; the normalized
    // output includes it, and the media parts are carried as metrics instead.
    usage.tokens.output_tokens = Some(
        candidates
            .saturating_sub(image)
            .saturating_sub(audio)
            .saturating_add(thoughts),
    );
    usage.tokens.cached_input_tokens = Some(cached);
    if thoughts > 0 {
        usage.tokens.reasoning_tokens = Some(thoughts);
    }
    metric(&mut usage, "image_output_tokens", image);
    metric(&mut usage, "audio_output_tokens", audio);
    if let Some(tier) = metadata.get("serviceTier").and_then(Value::as_str) {
        usage.dimensions.insert("service_tier".into(), tier.into());
        usage.actual_service_tier = Some(tier.to_owned());
    }
    Some(usage)
}

/// An embedding reply reports only the input side.
fn from_embedding(metadata: &Value) -> Option<NormalizedUsage> {
    let prompt = count(metadata, "promptTokenCount")?;
    let mut usage = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    usage.tokens.input_tokens = Some(prompt);
    Some(usage)
}

impl UsageExtractor for Aistudio {
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
        let Some(metadata) = body.get("usageMetadata") else {
            return Ok(None);
        };
        let mut usage = match context.operation.operation {
            Operation::CreateEmbedding | Operation::BatchCreateEmbedding => {
                from_embedding(metadata)
            }
            _ => from_metadata(metadata),
        };
        if let Some(usage) = usage.as_mut()
            && let Some(tier) = context
                .response
                .headers
                .get(SERVICE_TIER)
                .and_then(|value| value.to_str().ok())
        {
            usage.dimensions.insert("service_tier".into(), tier.into());
            usage.actual_service_tier = Some(tier.to_owned());
        }
        Ok(usage)
    }
}

/// How the records of one `streamGenerateContent` response arrive.
enum Records {
    Sse(SseDecoder),
    JsonArray(JsonArrayDecoder),
}

struct GeminiObserver {
    records: Records,
    tier: Option<String>,
    usage: Option<NormalizedUsage>,
}

impl GeminiObserver {
    fn see(&mut self, record: &Value) {
        let Some(metadata) = record.get("usageMetadata") else {
            return;
        };
        let Some(mut usage) = from_metadata(metadata) else {
            return;
        };
        if let Some(tier) = self.tier.clone() {
            usage.dimensions.insert("service_tier".into(), tier.clone());
            usage.actual_service_tier = Some(tier);
        }
        self.usage = Some(usage);
    }
}

impl UsageObserver for GeminiObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        let UsageFrame::HttpChunk(chunk) = frame else {
            return Ok(());
        };
        let invalid = |error: gproxy_protocol::codec::CodecError| {
            ChannelError::InvalidResponse(error.to_string())
        };
        match &mut self.records {
            Records::Sse(decoder) => {
                for frame in decoder.push(chunk).map_err(invalid)? {
                    if let SseFrame::Event(event) = frame
                        && let Ok(record) = serde_json::from_str::<Value>(&event.data)
                    {
                        self.see(&record);
                    }
                }
            }
            Records::JsonArray(decoder) => {
                for record in decoder.push(chunk).map_err(invalid)? {
                    self.see(&record);
                }
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

impl UsageStream for Aistudio {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        let UsageTransport::Http { framing } = context.transport else {
            return Err(ChannelError::InvalidResponse(
                "AI Studio serves no WebSocket stream".into(),
            ));
        };
        if context.operation.dialect.family() == WireFamily::OpenAi {
            return openai_wire::observer(context.operation.operation, context.operation.dialect);
        }
        let records = match framing {
            Some(StreamFraming::Sse) => Records::Sse(SseDecoder::new(LIMITS)),
            // `streamGenerateContent` without `alt=sse` answers with one
            // incrementally delivered JSON array.
            Some(StreamFraming::JsonArray) | None => {
                Records::JsonArray(JsonArrayDecoder::new(LIMITS))
            }
            Some(other) => {
                return Err(ChannelError::InvalidResponse(format!(
                    "Gemini streams are SSE or a JSON array, not {other:?}"
                )));
            }
        };
        let tier = context
            .headers
            .get(SERVICE_TIER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        Ok(Box::new(GeminiObserver {
            records,
            tier,
            usage: None,
        }))
    }
}
