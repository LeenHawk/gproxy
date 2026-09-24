//! Per-call metering from the `usageMetadata` of a Code Assist reply.
//!
//! The reply is a Gemini `GenerateContentResponse` nested under `response`,
//! so the counts are the ordinary Gemini ones (v3
//! `shared/gemini/usage.rs`): `promptTokenCount` already includes the cached
//! prefix, so the cached part is subtracted out to obtain the normalized
//! input; `candidatesTokenCount` excludes thoughts, so the two are added to
//! obtain the normalized output, and the image and audio modality
//! breakdowns are kept as metrics that are subsets of it, the way reasoning
//! is a subset rather than an addition.

use super::{invalid_response, unwrap_value};
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageFrame, UsageObserver,
    UsageStreamEnd,
};
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseFrame};
use gproxy_protocol::wire::gemini::{Modality, ModalityTokenCount, UsageMetadata};
use rust_decimal::Decimal;
use serde_json::Value;

/// Bounds for watching a Code Assist SSE stream; the host enforces the real
/// transfer limits, this only keeps the observer's buffers finite.
pub(crate) const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: u64::MAX,
    max_value_bytes: u64::MAX,
    max_body_bytes: u64::MAX,
    max_line_bytes: u64::MAX,
    max_part_bytes: 0,
    max_parts: 0,
};

fn count(value: Option<i64>) -> u64 {
    value
        .map(|value| u64::try_from(value).unwrap_or(0))
        .unwrap_or(0)
}

fn modality(details: Option<&Vec<ModalityTokenCount>>, wanted: Modality) -> u64 {
    details
        .into_iter()
        .flatten()
        .filter(|detail| detail.modality.as_ref() == Some(&wanted))
        .map(|detail| count(detail.token_count))
        .sum()
}

/// `usageMetadata` -> normalized. Both token totals must be present for the
/// reading to count; `None` means the reply reported no usage, not zero.
pub(crate) fn from_metadata(metadata: &UsageMetadata) -> Option<NormalizedUsage> {
    let prompt = u64::try_from(metadata.prompt_token_count?).ok()?;
    let candidates = u64::try_from(metadata.candidates_token_count?).ok()?;
    let cached = count(metadata.cached_content_token_count).min(prompt);
    let thoughts = count(metadata.thoughts_token_count);
    let image = modality(metadata.candidates_tokens_details.as_ref(), Modality::Image);
    let audio = modality(metadata.candidates_tokens_details.as_ref(), Modality::Audio);
    let mut usage = NormalizedUsage::default();
    // Gemini's promptTokenCount includes the cache read; the normalized
    // input excludes it.
    usage.tokens.input_tokens = Some(prompt - cached);
    usage.tokens.cached_input_tokens = Some(cached);
    usage.tokens.output_tokens = Some(candidates.saturating_add(thoughts));
    usage.tokens.reasoning_tokens = Some(thoughts);
    for (name, value) in [
        ("image_output_tokens", image),
        ("audio_output_tokens", audio),
        (
            "tool_use_prompt_tokens",
            count(metadata.tool_use_prompt_token_count),
        ),
    ] {
        if value > 0 {
            usage.metrics.insert(name.into(), Decimal::from(value));
        }
    }
    if let Some(tier) = metadata
        .service_tier
        .as_ref()
        .and_then(|tier| serde_json::to_value(tier).ok())
        .and_then(|tier| tier.as_str().map(str::to_owned))
    {
        usage.dimensions.insert("service_tier".into(), tier.clone());
        usage.actual_service_tier = Some(tier);
    }
    usage.completeness = UsageCompleteness::Complete;
    Some(usage)
}

/// The `usageMetadata` of a Code Assist reply, wrapped or already unwrapped.
pub(crate) fn from_value(value: &Value) -> Option<NormalizedUsage> {
    let metadata = unwrap_value(value).get("usageMetadata")?;
    from_metadata(&serde_json::from_value(metadata.clone()).ok()?)
}

pub(crate) fn buffered(
    context: &UsageContext<'_>,
) -> Result<Option<NormalizedUsage>, ChannelError> {
    if !context.response.status.is_success() {
        return Ok(None);
    }
    let Ok(value) = serde_json::from_slice::<Value>(context.response.body) else {
        return Ok(None);
    };
    Ok(from_value(&value))
}

/// Watches the `alt=sse` stream. Each chunk carries a whole reply, and the
/// counts are cumulative rather than deltas, so the last chunk that reports
/// any usage is the reading.
pub(crate) struct SseUsageObserver {
    sse: SseDecoder,
    latest: Option<NormalizedUsage>,
}

impl SseUsageObserver {
    pub(crate) fn new() -> Self {
        Self {
            sse: SseDecoder::new(SSE_LIMITS),
            latest: None,
        }
    }
}

impl UsageObserver for SseUsageObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        let UsageFrame::HttpChunk(chunk) = frame else {
            return Ok(());
        };
        let frames = self
            .sse
            .push(chunk)
            .map_err(|error| invalid_response(error.to_string()))?;
        for frame in frames {
            if let SseFrame::Event(event) = frame
                && let Ok(value) = serde_json::from_str::<Value>(&event.data)
                && let Some(usage) = from_value(&value)
            {
                self.latest = Some(usage);
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> Option<NormalizedUsage> {
        self.latest.clone()
    }

    fn finish(
        self: Box<Self>,
        end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        Ok(self.latest.map(|mut usage| {
            if end == UsageStreamEnd::Interrupted {
                usage.completeness = UsageCompleteness::Partial;
            }
            usage
        }))
    }
}
