//! Shared OpenAI buffered usage extraction.

use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::Operation;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr as _;

pub(crate) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    if ctx.key.operation == Operation::CreateSpeech {
        return speech(ctx.request_body, ctx.response_headers, ctx.response_body);
    }
    let value = serde_json::from_slice::<Value>(ctx.response_body).ok()?;
    match ctx.key.operation {
        Operation::CreateImage | Operation::EditImage => from_image_value(&value),
        Operation::CreateTranscription => from_transcription_value(&value),
        Operation::RetrieveVideo => from_video_value(&value),
        _ => value.get("usage").and_then(from_usage),
    }
}

pub(crate) fn speech(
    request: &[u8],
    headers: &http::HeaderMap,
    response: &[u8],
) -> Option<NormalizedUsage> {
    let format = serde_json::from_slice::<Value>(request)
        .ok()
        .and_then(|request| {
            request
                .get("response_format")
                .or_else(|| request.get("format"))
                .or_else(|| request.pointer("/output_format/codec"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| {
            headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(
                    |value| match value.split(';').next().unwrap_or(value).trim() {
                        "audio/pcm" | "audio/L16" => Some("pcm".to_owned()),
                        "audio/wav" | "audio/x-wav" => Some("wav".to_owned()),
                        _ => None,
                    },
                )
        })?;
    let (bytes, bytes_per_second) = match format.as_str() {
        "pcm" => (response.len() as u64, 48_000_u64),
        "wav" => wav_duration_parts(response)?,
        _ => return None,
    };
    let seconds = Decimal::from(bytes) / Decimal::from(bytes_per_second);
    let mut usage = NormalizedUsage::default();
    usage.metrics.insert("audio_seconds".into(), seconds);
    Some(usage)
}

fn wav_duration_parts(bytes: &[u8]) -> Option<(u64, u64)> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let byte_rate = u32::from_le_bytes(bytes.get(28..32)?.try_into().ok()?) as u64;
    if byte_rate == 0 {
        return None;
    }
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes.get(offset + 4..offset + 8)?.try_into().ok()?) as usize;
        if bytes.get(offset..offset + 4)? == b"data" {
            return Some((
                size.min(bytes.len().saturating_sub(offset + 8)) as u64,
                byte_rate,
            ));
        }
        offset = offset.checked_add(8 + size + (size % 2))?;
    }
    None
}

pub(super) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    if number(usage, "prompt_tokens") && number(usage, "completion_tokens") {
        Some(chat_usage(usage))
    } else if number(usage, "input_tokens") && number(usage, "output_tokens") {
        Some(responses_usage(usage))
    } else if number(usage, "prompt_tokens") && number(usage, "total_tokens") {
        Some(chat_usage(usage))
    } else {
        None
    }
}

pub(super) fn from_image_value(value: &Value) -> Option<NormalizedUsage> {
    let usage = value.get("usage")?;
    let mut normalized = from_usage(usage)?;
    let explicit = output_details(usage)
        .filter(|details| number(details, "image_tokens"))
        .map(|details| field(details, "image_tokens"));
    let image_tokens = explicit.unwrap_or(normalized.output_tokens);
    normalized.output_tokens = normalized.output_tokens.saturating_sub(image_tokens);
    normalized
        .metrics
        .insert("image_output_tokens".into(), Decimal::from(image_tokens));
    if let Some(outputs) = value.get("data").and_then(Value::as_array) {
        normalized
            .metrics
            .insert("image_outputs".into(), Decimal::from(outputs.len()));
    }
    Some(normalized)
}

pub(super) fn from_transcription_value(value: &Value) -> Option<NormalizedUsage> {
    let usage = value.get("usage")?;
    let token_usage = usage.get("type").and_then(Value::as_str) != Some("duration")
        && number(usage, "input_tokens")
        && number(usage, "output_tokens");
    let seconds = usage.get("seconds").and_then(decimal);
    if !token_usage && seconds.is_none() {
        return None;
    }
    let mut normalized = NormalizedUsage {
        input_tokens: field(usage, "input_tokens"),
        output_tokens: field(usage, "output_tokens"),
        ..Default::default()
    };
    if let Some(seconds) = seconds {
        normalized.metrics.insert("audio_seconds".into(), seconds);
    }
    Some(normalized)
}

fn chat_usage(usage: &Value) -> NormalizedUsage {
    let mut normalized = NormalizedUsage {
        input_tokens: field(usage, "prompt_tokens"),
        output_tokens: field(usage, "completion_tokens"),
        cached_input_tokens: usage
            .get("prompt_tokens_details")
            .map(|details| field(details, "cached_tokens"))
            .unwrap_or_default(),
        ..Default::default()
    };
    add_details(
        &mut normalized,
        usage.get("prompt_tokens_details"),
        usage.get("completion_tokens_details"),
        usage,
    );
    normalized
}

fn responses_usage(usage: &Value) -> NormalizedUsage {
    let mut normalized = NormalizedUsage {
        input_tokens: field(usage, "input_tokens"),
        output_tokens: field(usage, "output_tokens"),
        cached_input_tokens: usage
            .get("input_tokens_details")
            .map(|details| field(details, "cached_tokens"))
            .unwrap_or_default(),
        ..Default::default()
    };
    add_details(
        &mut normalized,
        usage.get("input_tokens_details"),
        usage.get("output_tokens_details"),
        usage,
    );
    normalized
}

fn add_details(
    normalized: &mut NormalizedUsage,
    input: Option<&Value>,
    output: Option<&Value>,
    usage: &Value,
) {
    for (name, value) in [
        (
            "cache_write_tokens",
            input.map(|v| field(v, "cache_write_tokens")),
        ),
        (
            "audio_input_tokens",
            input.map(|v| field(v, "audio_tokens")),
        ),
        (
            "reasoning_tokens",
            output.map(|v| field(v, "reasoning_tokens")),
        ),
        (
            "audio_output_tokens",
            output.map(|v| field(v, "audio_tokens")),
        ),
        (
            "web_searches",
            usage
                .get("server_tool_use_details")
                .or_else(|| usage.get("server_tool_use"))
                .map(|v| field(v, "web_search_requests")),
        ),
    ] {
        if let Some(value) = value.filter(|value| *value > 0) {
            normalized.metrics.insert(name.into(), Decimal::from(value));
        }
    }
}

fn from_video_value(value: &Value) -> Option<NormalizedUsage> {
    let mut normalized = NormalizedUsage::default();
    let mut measured = false;
    if let Some(tokens) = value.pointer("/usage/video_tokens").and_then(Value::as_u64) {
        measured = true;
        normalized
            .metrics
            .insert("video_tokens".into(), Decimal::from(tokens));
    }
    let seconds = value
        .pointer("/usage/seconds")
        .or_else(|| value.get("seconds"))
        .or_else(|| value.get("duration"))
        .and_then(decimal);
    if let Some(seconds) = seconds {
        measured = true;
        normalized.metrics.insert("video_seconds".into(), seconds);
    }
    for name in ["resolution", "size", "quality"] {
        if let Some(value) = value.get(name).and_then(Value::as_str) {
            normalized.dimensions.insert(name.into(), value.into());
        }
    }
    for name in ["with_audio", "generate_audio"] {
        if let Some(value) = value.get(name).and_then(Value::as_bool) {
            normalized
                .dimensions
                .insert("with_audio".into(), value.to_string());
        }
    }
    measured.then_some(normalized)
}

fn output_details(usage: &Value) -> Option<&Value> {
    if number(usage, "prompt_tokens") && number(usage, "completion_tokens") {
        usage.get("completion_tokens_details")
    } else if number(usage, "input_tokens") && number(usage, "output_tokens") {
        usage.get("output_tokens_details")
    } else {
        None
    }
}

fn field(value: &Value, name: &str) -> u64 {
    value.get(name).and_then(Value::as_u64).unwrap_or_default()
}

fn number(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(Value::is_u64)
}

fn decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(value) => Decimal::from_str(&value.to_string()).ok(),
        Value::String(value) => Decimal::from_str(value).ok(),
        _ => None,
    }
}
