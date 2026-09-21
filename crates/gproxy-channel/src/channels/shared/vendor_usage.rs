//! Usage as the three vendors report it, for channels that forward a vendor's
//! own wire unchanged.
//!
//! A cloud reseller (Azure, Vertex) does not invent a usage block: the body it
//! returns is the vendor's, so the counts are read with the vendor's field
//! names. A `custom` provider is the same case with the vendor unnamed — the
//! operator's `base_url` decides which of the four wires comes back, and the
//! dialect the exchange spoke is what says which to read. Only parsing lives
//! here; which dialect a response is in, and whether an operation is metered
//! at all, stays with the channel that asks.
//!
//! Field names come from the vendor references in `upstream_docs/`: OpenAI
//! Responses `usage`, OpenAI Chat Completions `usage`, Claude Messages `usage`
//! and Gemini `usageMetadata`. This is the only one of the three shared usage
//! readers that reads all four; `shared::openai_wire` reads the two OpenAI
//! shapes with the OpenAI platform's own extras, and
//! `shared::compatible::usage` reads three of them behind a per-channel
//! `Enrich` hook. The arithmetic agrees wherever they overlap on a
//! well-formed body — what differs is which fields exist and what an absent
//! one means, so none of the three substitutes for another.
//!
//! Here an absent count stays `None` rather than becoming zero, and a block
//! naming either side is read: a body reporting only `output_tokens` is
//! metered on its output alone.

use crate::channel::{ChannelError, NormalizedUsage, UsageCompleteness};
use gproxy_protocol::{Dialect, WireFamily};
use serde_json::Value;

/// Cap on how much of an accumulated stream is scanned for usage events. The
/// host already enforces the real transfer limits; this only bounds the work
/// a metering pass does on a body that turned out not to be JSON.
const MAX_SSE_EVENTS: usize = 4096;

/// Usage from one buffered response body, or from an accumulated SSE stream.
///
/// The host hands the extractor whatever the exchange accumulated: a complete
/// JSON document for a buffered answer, the raw `text/event-stream` bytes for
/// a streamed one. Both are covered, because a channel that only understood
/// JSON would silently meter nothing for every streaming request.
///
/// `None` means the response reported no usage, which is not the same as zero.
pub(crate) fn from_body(
    dialect: Dialect,
    body: &[u8],
) -> Result<Option<NormalizedUsage>, ChannelError> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        return Ok(from_value(dialect, &value));
    }
    Ok(from_event_stream(dialect, body))
}

/// Walk `data:` lines and merge what each event reports. Claude splits its
/// counts across `message_start` (input, cache) and `message_delta` (output);
/// the other dialects repeat a complete block, so a later value supersedes an
/// earlier one either way.
fn from_event_stream(dialect: Dialect, body: &[u8]) -> Option<NormalizedUsage> {
    let text = std::str::from_utf8(body).ok()?;
    let mut merged: Option<NormalizedUsage> = None;
    let mut seen = 0usize;
    for line in text.lines() {
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        seen += 1;
        if seen > MAX_SSE_EVENTS {
            break;
        }
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        if let Some(usage) = from_value(dialect, &value) {
            merged = Some(match merged {
                Some(previous) => merge(previous, usage),
                None => usage,
            });
        }
    }
    merged
}

/// Later counts win field by field; an absent field leaves the earlier one.
fn merge(mut into: NormalizedUsage, from: NormalizedUsage) -> NormalizedUsage {
    fn take(target: &mut Option<u64>, value: Option<u64>) {
        if value.is_some() {
            *target = value;
        }
    }
    take(&mut into.tokens.input_tokens, from.tokens.input_tokens);
    take(&mut into.tokens.output_tokens, from.tokens.output_tokens);
    take(
        &mut into.tokens.cached_input_tokens,
        from.tokens.cached_input_tokens,
    );
    take(
        &mut into.tokens.cache_creation_5m_tokens,
        from.tokens.cache_creation_5m_tokens,
    );
    take(
        &mut into.tokens.cache_creation_1h_tokens,
        from.tokens.cache_creation_1h_tokens,
    );
    take(
        &mut into.tokens.reasoning_tokens,
        from.tokens.reasoning_tokens,
    );
    into.metrics.extend(from.metrics);
    if from.actual_service_tier.is_some() {
        into.actual_service_tier = from.actual_service_tier;
    }
    into
}

/// The usage block inside one response or one stream event, wherever that
/// dialect puts it, turned into normalized counts.
fn from_value(dialect: Dialect, value: &Value) -> Option<NormalizedUsage> {
    match dialect.family() {
        WireFamily::Gemini => gemini(value.get("usageMetadata")?),
        WireFamily::OpenAi => {
            let usage = value
                .get("usage")
                .or_else(|| value.pointer("/response/usage"))?;
            match dialect {
                // Chat Completions names its counts prompt/completion; the
                // Responses shape names them input/output.
                Dialect::OpenAiChat => openai_chat(usage),
                _ => openai_responses(usage),
            }
        }
        WireFamily::Claude => {
            let usage = value
                .get("usage")
                .or_else(|| value.pointer("/message/usage"))?;
            claude(usage)
        }
    }
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

/// OpenAI Responses: `input_tokens` counts cache reads, the normalized input
/// does not.
fn openai_responses(usage: &Value) -> Option<NormalizedUsage> {
    let input = number(usage, "input_tokens");
    let output = number(usage, "output_tokens");
    if input.is_none() && output.is_none() {
        return None;
    }
    let cached = usage
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..Default::default()
    };
    normalized.tokens.input_tokens = input.map(|i| i.saturating_sub(cached.unwrap_or(0)));
    normalized.tokens.output_tokens = output;
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.reasoning_tokens = usage
        .pointer("/output_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64);
    Some(normalized)
}

/// OpenAI Chat Completions: `prompt_tokens` counts cache reads, the normalized
/// input does not.
fn openai_chat(usage: &Value) -> Option<NormalizedUsage> {
    let input = number(usage, "prompt_tokens");
    let output = number(usage, "completion_tokens");
    if input.is_none() && output.is_none() {
        return None;
    }
    let cached = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..Default::default()
    };
    normalized.tokens.input_tokens = input.map(|i| i.saturating_sub(cached.unwrap_or(0)));
    normalized.tokens.output_tokens = output;
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.reasoning_tokens = usage
        .pointer("/completion_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64);
    Some(normalized)
}

/// Claude Messages: `input_tokens` already excludes cache reads and cache
/// creation, so nothing is subtracted. The per-TTL breakdown appears under
/// `cache_creation` when the request used more than one breakpoint TTL.
fn claude(usage: &Value) -> Option<NormalizedUsage> {
    let input = number(usage, "input_tokens");
    let output = number(usage, "output_tokens");
    if input.is_none() && output.is_none() {
        return None;
    }
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..Default::default()
    };
    normalized.tokens.input_tokens = input;
    normalized.tokens.output_tokens = output;
    normalized.tokens.cached_input_tokens = number(usage, "cache_read_input_tokens");
    let five_minute = usage
        .pointer("/cache_creation/ephemeral_5m_input_tokens")
        .and_then(Value::as_u64);
    let one_hour = usage
        .pointer("/cache_creation/ephemeral_1h_input_tokens")
        .and_then(Value::as_u64);
    // Without the breakdown the total is the default five-minute TTL.
    normalized.tokens.cache_creation_5m_tokens = match (five_minute, one_hour) {
        (None, None) => number(usage, "cache_creation_input_tokens"),
        (five, _) => five,
    };
    normalized.tokens.cache_creation_1h_tokens = one_hour;
    Some(normalized)
}

/// Gemini: `promptTokenCount` counts cached content, the normalized input does
/// not. `candidatesTokenCount` excludes thoughts, which the normalized output
/// includes, so the two are added and thoughts recorded as the reasoning
/// subset.
fn gemini(usage: &Value) -> Option<NormalizedUsage> {
    let prompt = number(usage, "promptTokenCount");
    let candidates = number(usage, "candidatesTokenCount");
    let thoughts = number(usage, "thoughtsTokenCount");
    if prompt.is_none() && candidates.is_none() && thoughts.is_none() {
        return None;
    }
    let cached = number(usage, "cachedContentTokenCount");
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..Default::default()
    };
    normalized.tokens.input_tokens = prompt.map(|p| p.saturating_sub(cached.unwrap_or(0)));
    normalized.tokens.output_tokens = match (candidates, thoughts) {
        (None, None) => None,
        (candidates, thoughts) => Some(
            candidates
                .unwrap_or(0)
                .saturating_add(thoughts.unwrap_or(0)),
        ),
    };
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.reasoning_tokens = thoughts;
    if let Some(count) = number(usage, "toolUsePromptTokenCount") {
        normalized
            .metrics
            .insert("tool_use_prompt_tokens".into(), count.into());
    }
    Some(normalized)
}
