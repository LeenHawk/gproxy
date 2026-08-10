//! Per-protocol usage extraction over loose JSON (`serde_json::Value`).
//!
//! Tolerant by design: missing numeric fields read as 0, but a body/frame with
//! no usage-bearing structure at all yields `None`. Subtractions are
//! saturating so a malformed `cached > prompt` never underflows.

use serde_json::Value;

use super::NormalizedUsage;
use crate::protocol::operation::{ContentGenerationKind, Provider};
use crate::transform::common::sse::SseFrame;

/// Extract usage from a NON-streaming response body of the given family.
///
/// For [`Provider::OpenAi`] both wire shapes are handled: chat completions
/// (`prompt_tokens`) is tried first, then responses (`input_tokens`) — the
/// field names are disjoint, so there is no ambiguity.
pub fn from_response(family: Provider, body: &Value) -> Option<NormalizedUsage> {
    match family {
        Provider::Claude => {
            let usage = body.get("usage").filter(|u| u.is_object())?;
            if !numeric(usage, "input_tokens") || !numeric(usage, "output_tokens") {
                return None;
            }
            Some(claude_usage(usage))
        }
        Provider::OpenAi => {
            let usage = body.get("usage").filter(|u| u.is_object())?;
            openai_usage(usage)
        }
        Provider::Gemini => {
            let meta = body.get("usageMetadata").filter(|m| m.is_object())?;
            if !numeric(meta, "promptTokenCount") || !numeric(meta, "candidatesTokenCount") {
                return None;
            }
            Some(gemini_usage(meta))
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

/// Extract usage for a provider-shaped image response.
///
/// OpenAI-compatible image endpoints are inconsistent about output modality
/// detail: some report `image_tokens`, while OpenRouter's image API may expose
/// only aggregate completion/output tokens. For an image operation the latter
/// are image-output tokens, so move the aggregate out of ordinary output when
/// no explicit image breakdown is present. An explicit zero remains
/// authoritative and is not replaced by the aggregate.
pub fn from_image_response(family: Provider, body: &Value) -> Option<NormalizedUsage> {
    let mut normalized = from_response(family, body)?;
    if family == Provider::OpenAi {
        let usage = body.get("usage").filter(|u| u.is_object())?;
        if !openai_image_output_is_explicit(usage) {
            normalized.image_output = normalized.output;
            normalized.output = 0;
        }
    }
    Some(normalized)
}

/// Extract the final usage-bearing event from a provider-shaped image SSE
/// response. OpenAI Images puts `usage` on its `*.completed` event.
pub fn from_image_stream_frames(family: Provider, frames: &[SseFrame]) -> Option<NormalizedUsage> {
    frames.iter().rev().find_map(|frame| {
        let body = frame_json(frame)?;
        from_image_response(family, &body)
    })
}

/// Extract the FINAL usage from buffered stream frames.
///
/// Walks the decoded SSE frames from the END backwards and returns the first
/// frame yielding usage per the family's stream shape. The claude path merges
/// `message_start` (input side, frame ~1, found by a forward scan) with the
/// LAST `message_delta` carrying usage. Native Claude deltas normally contain
/// only cumulative output; channel-normalized providers such as Bedrock can
/// report the final input/cache side there as well. An aggregate-only delta
/// must not erase a TTL breakdown already present in `message_start`.
pub fn from_stream_frames(
    kind: ContentGenerationKind,
    frames: &[SseFrame],
) -> Option<NormalizedUsage> {
    match kind {
        ContentGenerationKind::ClaudeMessages => claude_stream(frames),
        ContentGenerationKind::OpenAiChatCompletions => frames.iter().rev().find_map(|frame| {
            let json = frame_json(frame)?;
            let usage = json.get("usage").filter(|u| u.is_object())?;
            openai_usage(usage)
        }),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => {
            frames.iter().rev().find_map(|frame| {
                let json = frame_json(frame)?;
                let is_completed = frame.event.as_deref() == Some("response.completed")
                    || json.get("type").and_then(Value::as_str) == Some("response.completed");
                if !is_completed {
                    return None;
                }
                let usage = json
                    .get("response")?
                    .get("usage")
                    .filter(|u| u.is_object())?;
                openai_usage(usage)
            })
        }
        ContentGenerationKind::GeminiGenerateContent => frames.iter().rev().find_map(|frame| {
            let json = frame_json(frame)?;
            let meta = json.get("usageMetadata").filter(|m| m.is_object())?;
            (numeric(meta, "promptTokenCount") && numeric(meta, "candidatesTokenCount"))
                .then(|| gemini_usage(meta))
        }),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn frame_json(frame: &SseFrame) -> Option<Value> {
    serde_json::from_str(&frame.data).ok()
}

/// Tolerant numeric field read: missing / non-numeric = 0.
fn field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn numeric(value: &Value, key: &str) -> bool {
    value.get(key).is_some_and(Value::is_u64)
}

/// Claude `usage` object. `input_tokens` already excludes cache parts (claude
/// separates natively). `cache_creation` preserves the 5m/1h breakdown when
/// present; the legacy aggregate `cache_creation_input_tokens` is recorded as
/// 5m because older Claude responses did not expose a TTL split.
fn claude_usage(usage: &Value) -> NormalizedUsage {
    let (cache_creation_5m, cache_creation_1h) =
        match usage.get("cache_creation").filter(|v| v.is_object()) {
            Some(breakdown) => (
                field(breakdown, "ephemeral_5m_input_tokens"),
                field(breakdown, "ephemeral_1h_input_tokens"),
            ),
            None => (field(usage, "cache_creation_input_tokens"), 0),
        };
    NormalizedUsage {
        input: field(usage, "input_tokens"),
        output: field(usage, "output_tokens"),
        image_output: 0,
        cache_read: field(usage, "cache_read_input_tokens"),
        cache_creation_5m,
        cache_creation_30m: 0,
        cache_creation_1h,
        reasoning: 0,
    }
}

/// OpenAI `usage` object, either wire shape (disjoint field names).
fn openai_usage(usage: &Value) -> Option<NormalizedUsage> {
    if numeric(usage, "prompt_tokens") && numeric(usage, "completion_tokens") {
        Some(openai_chat_usage(usage))
    } else if numeric(usage, "input_tokens") && numeric(usage, "output_tokens") {
        Some(openai_responses_usage(usage))
    } else if numeric(usage, "prompt_tokens") && numeric(usage, "total_tokens") {
        // Embeddings report input-only usage as prompt + total, without a
        // completion field. Reuse chat input/cache normalization; its missing
        // completion side is intentionally zero.
        Some(openai_chat_usage(usage))
    } else {
        None
    }
}

/// OpenAI chat completions: `prompt_tokens` INCLUDES cache reads/writes → subtract.
/// GPT-5.6+ reports explicit/implicit cache writes as `cache_write_tokens`.
fn openai_chat_usage(usage: &Value) -> NormalizedUsage {
    let prompt = field(usage, "prompt_tokens");
    let completion = field(usage, "completion_tokens");
    let (cached, cache_write) = usage.get("prompt_tokens_details").map_or((0, 0), |d| {
        (field(d, "cached_tokens"), field(d, "cache_write_tokens"))
    });
    let (reasoning, reported_image_output) =
        usage.get("completion_tokens_details").map_or((0, 0), |d| {
            (field(d, "reasoning_tokens"), field(d, "image_tokens"))
        });
    let image_output = reported_image_output.min(completion);
    NormalizedUsage {
        input: prompt.saturating_sub(cached).saturating_sub(cache_write),
        output: completion.saturating_sub(image_output),
        image_output,
        cache_read: cached,
        cache_creation_30m: cache_write,
        reasoning,
        ..Default::default()
    }
}

/// OpenAI responses: `input_tokens` INCLUDES cache reads/writes → subtract.
/// GPT-5.6+ reports explicit/implicit cache writes as `cache_write_tokens`.
fn openai_responses_usage(usage: &Value) -> NormalizedUsage {
    let input = field(usage, "input_tokens");
    let output = field(usage, "output_tokens");
    let (cached, cache_write) = usage.get("input_tokens_details").map_or((0, 0), |d| {
        (field(d, "cached_tokens"), field(d, "cache_write_tokens"))
    });
    let (reasoning, reported_image_output) =
        usage.get("output_tokens_details").map_or((0, 0), |d| {
            (field(d, "reasoning_tokens"), field(d, "image_tokens"))
        });
    let image_output = reported_image_output.min(output);
    NormalizedUsage {
        input: input.saturating_sub(cached).saturating_sub(cache_write),
        output: output.saturating_sub(image_output),
        image_output,
        cache_read: cached,
        cache_creation_30m: cache_write,
        reasoning,
        ..Default::default()
    }
}

fn openai_image_output_is_explicit(usage: &Value) -> bool {
    // Keep this selection in lockstep with `openai_usage`: mixed alias objects
    // occasionally contain both detail containers, and a field belonging to
    // the shape we did not normalize must not suppress the image fallback.
    let details = if numeric(usage, "prompt_tokens") && numeric(usage, "completion_tokens") {
        usage.get("completion_tokens_details")
    } else if numeric(usage, "input_tokens") && numeric(usage, "output_tokens") {
        usage.get("output_tokens_details")
    } else {
        None
    };
    details.is_some_and(|details| numeric(details, "image_tokens"))
}

/// Gemini `usageMetadata`. `promptTokenCount` INCLUDES cached → subtract.
///
/// Billing choice: gemini bills thinking as output and `totalTokenCount` is
/// often prompt + candidates + thoughts, with `candidatesTokenCount` NOT
/// including thoughts — so we set output = candidates + thoughts (billing
/// covers thinking) and record thoughts in the reasoning column (informational
/// subset of output, not double-billed).
fn gemini_usage(meta: &Value) -> NormalizedUsage {
    let prompt = field(meta, "promptTokenCount");
    let cached = field(meta, "cachedContentTokenCount");
    let candidates = field(meta, "candidatesTokenCount");
    let thoughts = field(meta, "thoughtsTokenCount");
    let reported_image_output = meta
        .get("candidatesTokensDetails")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|detail| {
            detail
                .get("modality")
                .and_then(Value::as_str)
                .is_some_and(|modality| modality.eq_ignore_ascii_case("image"))
        })
        .fold(0u64, |total, detail| {
            total.saturating_add(field(detail, "tokenCount"))
        });
    let image_output = reported_image_output.min(candidates);
    NormalizedUsage {
        input: prompt.saturating_sub(cached),
        output: candidates
            .saturating_sub(image_output)
            .saturating_add(thoughts),
        image_output,
        cache_read: cached,
        reasoning: thoughts,
        ..Default::default()
    }
}

/// Claude stream: start with input/cache from `message_start`, then overlay any
/// input/cache fields explicitly carried by the LAST `message_delta` and take
/// its cumulative output. A complete input/output pair is required so synthetic
/// placeholder events cannot be mistaken for authoritative usage.
fn claude_stream(frames: &[SseFrame]) -> Option<NormalizedUsage> {
    let start = frames.iter().find_map(|frame| {
        let json = frame_json(frame)?;
        if json.get("type").and_then(Value::as_str) != Some("message_start") {
            return None;
        }
        let usage = json
            .get("message")?
            .get("usage")
            .filter(|u| u.is_object())?;
        numeric(usage, "input_tokens").then(|| ClaudeStreamStart {
            usage: claude_usage(usage),
            has_cache_creation: numeric(usage, "cache_creation_input_tokens")
                || usage.get("cache_creation").is_some(),
        })
    });
    let delta = frames.iter().rev().find_map(|frame| {
        let json = frame_json(frame)?;
        if json.get("type").and_then(Value::as_str) != Some("message_delta") {
            return None;
        }
        let usage = json.get("usage").filter(|u| u.is_object())?;
        numeric(usage, "output_tokens").then(|| ClaudeStreamDelta {
            usage: claude_usage(usage),
            has_input: numeric(usage, "input_tokens"),
            has_cache_read: numeric(usage, "cache_read_input_tokens"),
            has_cache_creation: numeric(usage, "cache_creation_input_tokens")
                || usage.get("cache_creation").is_some(),
            has_cache_creation_breakdown: usage.get("cache_creation").is_some(),
        })
    });
    match (start, delta) {
        (Some(mut start), Some(delta)) => {
            start.usage.output = delta.usage.output;
            if delta.has_input {
                start.usage.input = delta.usage.input;
            }
            if delta.has_cache_read {
                start.usage.cache_read = delta.usage.cache_read;
            }
            if delta.has_cache_creation_breakdown
                || (delta.has_cache_creation && !start.has_cache_creation)
            {
                start.usage.cache_creation_5m = delta.usage.cache_creation_5m;
                start.usage.cache_creation_1h = delta.usage.cache_creation_1h;
            }
            Some(start.usage)
        }
        (Some(_), None) => None,
        (None, Some(only)) => only.has_input.then_some(only.usage),
        (None, None) => None,
    }
}

struct ClaudeStreamStart {
    usage: NormalizedUsage,
    has_cache_creation: bool,
}

struct ClaudeStreamDelta {
    usage: NormalizedUsage,
    has_input: bool,
    has_cache_read: bool,
    has_cache_creation: bool,
    has_cache_creation_breakdown: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_response_with_cache_breakdown() {
        let body = json!({
            "usage": {
                "input_tokens": 100,
                "output_tokens": 40,
                "cache_read_input_tokens": 300,
                "cache_creation_input_tokens": 999,
                "cache_creation": {
                    "ephemeral_5m_input_tokens": 50,
                    "ephemeral_1h_input_tokens": 20
                }
            }
        });
        let u = from_response(Provider::Claude, &body).unwrap();
        // input untouched (claude separates cache natively); breakdown wins.
        assert_eq!(u.input, 100);
        assert_eq!(u.output, 40);
        assert_eq!(u.cache_read, 300);
        assert_eq!(u.cache_creation_5m, 50);
        assert_eq!(u.cache_creation_1h, 20);
        assert_eq!(u.cache_creation(), 70);
        assert_eq!(u.total(), 510);
    }

    #[test]
    fn claude_response_with_legacy_cache_creation() {
        let body = json!({
            "usage": {
                "input_tokens": 10,
                "output_tokens": 4,
                "cache_read_input_tokens": 0,
                "cache_creation_input_tokens": 123
            }
        });
        let u = from_response(Provider::Claude, &body).unwrap();
        assert_eq!(u.cache_creation_5m, 123);
        assert_eq!(u.cache_creation_1h, 0);
        assert_eq!(u.cache_creation(), 123);
    }

    #[test]
    fn explicit_zero_usage_is_authoritative_but_placeholders_are_not() {
        let zero = json!({"usage": {"input_tokens": 0, "output_tokens": 0}});
        assert_eq!(
            from_response(Provider::Claude, &zero),
            Some(NormalizedUsage::default())
        );
        assert!(from_response(Provider::Claude, &json!({"usage": {}})).is_none());
        assert_eq!(
            from_response(
                Provider::OpenAi,
                &json!({"usage":{"prompt_tokens":0,"completion_tokens":0}}),
            ),
            Some(NormalizedUsage::default())
        );
        assert_eq!(
            from_response(
                Provider::Gemini,
                &json!({"usageMetadata":{"promptTokenCount":0,"candidatesTokenCount":0}}),
            ),
            Some(NormalizedUsage::default())
        );

        let frames = vec![
            SseFrame::event(
                "message_start",
                json!({"type":"message_start","message":{"usage":{"input_tokens":0,"output_tokens":0}}}).to_string(),
            ),
            SseFrame::event(
                "message_delta",
                json!({"type":"message_delta","usage":{"output_tokens":0}}).to_string(),
            ),
        ];
        assert_eq!(
            from_stream_frames(ContentGenerationKind::ClaudeMessages, &frames),
            Some(NormalizedUsage::default())
        );
    }

    #[test]
    fn openai_usage_cache_write() {
        let chat = json!({
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 200,
                "prompt_tokens_details": {
                    "cached_tokens": 600,
                    "cache_write_tokens": 150
                },
                "completion_tokens_details": {
                    "reasoning_tokens": 80,
                    "image_tokens": 50
                }
            }
        });
        let u = from_response(Provider::OpenAi, &chat).unwrap();
        assert_eq!(u.input, 250); // prompt - cache read - cache write
        assert_eq!(u.cache_read, 600);
        assert_eq!(u.cache_creation_30m, 150);
        assert_eq!(u.output, 150);
        assert_eq!(u.image_output, 50);
        assert_eq!(u.reasoning, 80);
        assert_eq!(u.cache_creation(), 150);
        assert_eq!(u.total(), 1200);

        // Missing details → cache 0, full input.
        let plain = json!({"usage": {"prompt_tokens": 1000, "completion_tokens": 200}});
        let u = from_response(Provider::OpenAi, &plain).unwrap();
        assert_eq!(u.input, 1000);
        assert_eq!(u.cache_read, 0);
        assert_eq!(u.output, 200);
        assert_eq!(u.image_output, 0);
        assert_eq!(u.reasoning, 0);

        let responses = json!({
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 200,
                "input_tokens_details": {
                    "cached_tokens": 600,
                    "cache_write_tokens": 150
                },
                "output_tokens_details": {
                    "reasoning_tokens": 80,
                    "image_tokens": 60
                }
            }
        });
        let u = from_response(Provider::OpenAi, &responses).unwrap();
        assert_eq!(u.input, 250);
        assert_eq!(u.cache_read, 600);
        assert_eq!(u.cache_creation_30m, 150);
        assert_eq!(u.output, 140);
        assert_eq!(u.image_output, 60);
        assert_eq!(u.reasoning, 80);
    }

    #[test]
    fn openai_image_usage_falls_back_to_aggregate_output_without_breakdown() {
        let chat = json!({
            "usage": {
                "prompt_tokens": 25,
                "completion_tokens": 1200,
                "total_tokens": 1225
            }
        });
        let u = from_image_response(Provider::OpenAi, &chat).unwrap();
        assert_eq!(u.input, 25);
        assert_eq!(u.output, 0);
        assert_eq!(u.image_output, 1200);
        assert_eq!(u.total(), 1225);

        // An explicit breakdown is authoritative, including an explicit zero.
        let explicit_zero = json!({
            "usage": {
                "input_tokens": 25,
                "output_tokens": 40,
                "output_tokens_details": {"image_tokens": 0}
            }
        });
        let u = from_image_response(Provider::OpenAi, &explicit_zero).unwrap();
        assert_eq!(u.output, 40);
        assert_eq!(u.image_output, 0);

        let responses = json!({
            "usage": {
                "input_tokens": 25,
                "output_tokens": 1040,
                "output_tokens_details": {
                    "text_tokens": 40,
                    "image_tokens": 1000
                }
            }
        });
        let u = from_image_response(Provider::OpenAi, &responses).unwrap();
        assert_eq!(u.output, 40);
        assert_eq!(u.image_output, 1000);

        let malformed = json!({
            "usage": {
                "input_tokens": 5,
                "output_tokens": 10,
                "output_tokens_details": {"image_tokens": 99}
            }
        });
        let u = from_image_response(Provider::OpenAi, &malformed).unwrap();
        assert_eq!(u.output, 0);
        assert_eq!(u.image_output, 10);
        assert_eq!(u.total(), 15);

        // An unrelated details alias must not suppress fallback for the chat
        // shape selected by `openai_usage`.
        let mixed_aliases = json!({
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 12,
                "input_tokens": 5,
                "output_tokens": 12,
                "output_tokens_details": {"image_tokens": 0}
            }
        });
        let u = from_image_response(Provider::OpenAi, &mixed_aliases).unwrap();
        assert_eq!(u.output, 0);
        assert_eq!(u.image_output, 12);

        let frames = [SseFrame::data(
            json!({
                "type": "image_generation.completed",
                "b64_json": "AAAA",
                "usage": {
                    "input_tokens": 25,
                    "output_tokens": 1000,
                    "total_tokens": 1025,
                    "output_tokens_details": {"image_tokens": 1000}
                }
            })
            .to_string(),
        )];
        let u = from_image_stream_frames(Provider::OpenAi, &frames).unwrap();
        assert_eq!(u.input, 25);
        assert_eq!(u.output, 0);
        assert_eq!(u.image_output, 1000);
    }

    #[test]
    fn openai_embedding_input_only_usage_is_authoritative() {
        let body = json!({
            "usage": {
                "prompt_tokens": 1000,
                "total_tokens": 1000,
                "prompt_tokens_details": {
                    "cached_tokens": 600,
                    "cache_write_tokens": 150
                }
            }
        });
        let u = from_response(Provider::OpenAi, &body).unwrap();
        assert_eq!(u.input, 250);
        assert_eq!(u.output, 0);
        assert_eq!(u.image_output, 0);
        assert_eq!(u.cache_read, 600);
        assert_eq!(u.cache_creation_30m, 150);
        assert_eq!(u.total(), 1000);
    }

    #[test]
    fn gemini_response_thoughts_and_cached() {
        let body = json!({
            "usageMetadata": {
                "promptTokenCount": 500,
                "candidatesTokenCount": 100,
                "cachedContentTokenCount": 200,
                "thoughtsTokenCount": 30,
                "candidatesTokensDetails": [
                    {"modality": "TEXT", "tokenCount": 30},
                    {"modality": "IMAGE", "tokenCount": 70}
                ],
                "totalTokenCount": 630
            }
        });
        let u = from_response(Provider::Gemini, &body).unwrap();
        assert_eq!(u.input, 300); // prompt - cached
        assert_eq!(u.output, 60); // non-image candidates + thoughts
        assert_eq!(u.image_output, 70);
        assert_eq!(u.reasoning, 30);
        assert_eq!(u.cache_read, 200);
        assert_eq!(u.total(), 630);

        let without_details = json!({
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 20,
                "thoughtsTokenCount": 5
            }
        });
        let u = from_response(Provider::Gemini, &without_details).unwrap();
        assert_eq!(u.output, 25);
        assert_eq!(u.image_output, 0);

        let malformed = json!({
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 20,
                "thoughtsTokenCount": 5,
                "candidatesTokensDetails": [
                    {"modality": "IMAGE", "tokenCount": 99}
                ]
            }
        });
        let u = from_response(Provider::Gemini, &malformed).unwrap();
        assert_eq!(u.output, 5);
        assert_eq!(u.image_output, 20);
        assert_eq!(u.total(), 35);
    }

    #[test]
    fn stream_frames_final_usage() {
        // Claude: message_start input + cumulative message_delta output (last wins).
        let frames = vec![
            SseFrame::event(
                "message_start",
                json!({"type": "message_start", "message": {"usage": {
                    "input_tokens": 25, "output_tokens": 1,
                    "cache_read_input_tokens": 10,
                    "cache_creation_input_tokens": 20,
                    "cache_creation": {
                        "ephemeral_5m_input_tokens": 0,
                        "ephemeral_1h_input_tokens": 20
                    }
                }}})
                .to_string(),
            ),
            SseFrame::event(
                "message_delta",
                json!({"type": "message_delta", "usage": {"output_tokens": 5}}).to_string(),
            ),
            SseFrame::event(
                "message_delta",
                json!({"type": "message_delta", "usage": {
                    "output_tokens": 12,
                    "cache_creation_input_tokens": 20
                }})
                .to_string(),
            ),
        ];
        let u = from_stream_frames(ContentGenerationKind::ClaudeMessages, &frames).unwrap();
        assert_eq!(u.input, 25);
        assert_eq!(u.cache_read, 10);
        assert_eq!(u.cache_creation_5m, 0);
        assert_eq!(u.cache_creation_1h, 20);
        assert_eq!(u.output, 12);

        // OpenAI chat: only the final chunk carries usage (include_usage).
        let frames = vec![
            SseFrame::data(json!({"choices": [{"delta": {"content": "hi"}}]}).to_string()),
            SseFrame::data(json!({"choices": [], "usage": null}).to_string()),
            SseFrame::data(
                json!({"choices": [], "usage": {"prompt_tokens": 7, "completion_tokens": 3}})
                    .to_string(),
            ),
            SseFrame::data("[DONE]"),
        ];
        let u = from_stream_frames(ContentGenerationKind::OpenAiChatCompletions, &frames).unwrap();
        assert_eq!(u.input, 7);
        assert_eq!(u.output, 3);

        // No usage anywhere → None.
        let frames = vec![
            SseFrame::data(json!({"choices": [{"delta": {"content": "x"}}]}).to_string()),
            SseFrame::data("[DONE]"),
        ];
        assert!(
            from_stream_frames(ContentGenerationKind::OpenAiChatCompletions, &frames).is_none()
        );
    }
}
