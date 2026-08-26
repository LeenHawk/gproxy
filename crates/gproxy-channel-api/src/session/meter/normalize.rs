use gproxy_protocol::openai::audio::AudioUsage;
use gproxy_protocol::openai::realtime::RealtimeUsage;
use rust_decimal::Decimal;

use crate::{ChannelError, NormalizedUsage};

pub(super) fn realtime(usage: &RealtimeUsage) -> Result<NormalizedUsage, ChannelError> {
    let input = usage
        .input_tokens
        .ok_or_else(|| decode("usage.input_tokens is missing"))?;
    let output = usage
        .output_tokens
        .ok_or_else(|| decode("usage.output_tokens is missing"))?;
    let total = input
        .checked_add(output)
        .ok_or_else(|| decode("usage token total overflows"))?;
    if usage.total_tokens.is_some_and(|reported| reported != total) {
        return Err(decode("usage.total_tokens disagrees with input and output"));
    }
    let cached = usage
        .input_token_details
        .as_ref()
        .and_then(|details| details.cached_tokens)
        .unwrap_or_default();
    if cached > input {
        return Err(decode("usage cached tokens exceed input tokens"));
    }
    let mut normalized = NormalizedUsage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        ..Default::default()
    };
    if let Some(details) = usage.input_token_details.as_ref() {
        for (name, value) in [
            ("text_input_tokens", details.text_tokens),
            ("audio_input_tokens", details.audio_tokens),
            ("image_input_tokens", details.image_tokens),
            ("cached_input_tokens", details.cached_tokens),
        ] {
            metric(&mut normalized, name, value);
        }
    }
    if let Some(details) = usage.output_token_details.as_ref() {
        metric(&mut normalized, "text_output_tokens", details.text_tokens);
        metric(&mut normalized, "audio_output_tokens", details.audio_tokens);
    }
    Ok(normalized)
}

pub(super) fn audio(usage: &AudioUsage) -> Result<NormalizedUsage, ChannelError> {
    match usage {
        AudioUsage::Tokens(usage) => {
            let total = usage
                .input_tokens
                .checked_add(usage.output_tokens)
                .ok_or_else(|| decode("transcription token total overflows"))?;
            if total != usage.total_tokens {
                return Err(decode(
                    "transcription total_tokens disagrees with input and output",
                ));
            }
            let mut normalized = NormalizedUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                ..Default::default()
            };
            if let Some(details) = usage.input_token_details.as_ref() {
                metric(&mut normalized, "audio_input_tokens", details.audio_tokens);
                metric(&mut normalized, "text_input_tokens", details.text_tokens);
            }
            Ok(normalized)
        }
        AudioUsage::Duration(usage) => {
            let seconds = usage
                .seconds
                .to_string()
                .parse::<Decimal>()
                .ok()
                .filter(|seconds| *seconds >= Decimal::ZERO)
                .ok_or_else(|| decode("transcription duration is not finite and nonnegative"))?;
            let mut normalized = NormalizedUsage::default();
            normalized.metrics.insert("audio_seconds".into(), seconds);
            Ok(normalized)
        }
        AudioUsage::Raw(_) => Err(decode("transcription usage has an unknown shape")),
    }
}

fn metric(usage: &mut NormalizedUsage, name: &str, value: Option<u64>) {
    if let Some(value) = value.filter(|value| *value > 0) {
        usage.metrics.insert(name.into(), Decimal::from(value));
    }
}

fn decode(message: impl Into<String>) -> ChannelError {
    ChannelError::Decode(format!("Realtime sideband: {}", message.into()))
}
