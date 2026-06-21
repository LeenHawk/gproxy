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
            Some(claude_usage(usage))
        }
        Provider::OpenAi => {
            let usage = body.get("usage").filter(|u| u.is_object())?;
            openai_usage(usage)
        }
        Provider::Gemini => {
            let meta = body.get("usageMetadata").filter(|m| m.is_object())?;
            Some(gemini_usage(meta))
        }
    }
}

/// Extract the FINAL usage from buffered stream frames.
///
/// Walks the decoded SSE frames from the END backwards and returns the first
/// frame yielding usage per the family's stream shape. The claude path merges
/// `message_start` (input side, frame ~1, found by a forward scan) with the
/// LAST `message_delta` carrying usage (cumulative output side).
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
        ContentGenerationKind::OpenAiResponses => frames.iter().rev().find_map(|frame| {
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
        }),
        ContentGenerationKind::GeminiGenerateContent => frames.iter().rev().find_map(|frame| {
            let json = frame_json(frame)?;
            let meta = json.get("usageMetadata").filter(|m| m.is_object())?;
            Some(gemini_usage(meta))
        }),
    }
}

fn frame_json(frame: &SseFrame) -> Option<Value> {
    serde_json::from_str(&frame.data).ok()
}

/// Tolerant numeric field read: missing / non-numeric = 0.
fn field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Claude `usage` object. `input_tokens` already excludes cache parts (claude
/// separates natively). `cache_creation` prefers the 5m/1h breakdown object
/// (fields summed) over the legacy aggregate `cache_creation_input_tokens`.
fn claude_usage(usage: &Value) -> NormalizedUsage {
    let cache_creation = match usage.get("cache_creation").filter(|v| v.is_object()) {
        Some(breakdown) => {
            field(breakdown, "ephemeral_5m_input_tokens")
                + field(breakdown, "ephemeral_1h_input_tokens")
        }
        None => field(usage, "cache_creation_input_tokens"),
    };
    NormalizedUsage {
        input: field(usage, "input_tokens"),
        output: field(usage, "output_tokens"),
        cache_read: field(usage, "cache_read_input_tokens"),
        cache_creation,
        reasoning: 0,
    }
}

/// OpenAI `usage` object, either wire shape (disjoint field names).
fn openai_usage(usage: &Value) -> Option<NormalizedUsage> {
    if usage.get("prompt_tokens").is_some() {
        Some(openai_chat_usage(usage))
    } else if usage.get("input_tokens").is_some() {
        Some(openai_responses_usage(usage))
    } else {
        None
    }
}

/// OpenAI chat completions: `prompt_tokens` INCLUDES cached → subtract.
/// OpenAI does not bill cache creation separately → cache_creation = 0.
fn openai_chat_usage(usage: &Value) -> NormalizedUsage {
    let prompt = field(usage, "prompt_tokens");
    let cached = usage
        .get("prompt_tokens_details")
        .map_or(0, |d| field(d, "cached_tokens"));
    let reasoning = usage
        .get("completion_tokens_details")
        .map_or(0, |d| field(d, "reasoning_tokens"));
    NormalizedUsage {
        input: prompt.saturating_sub(cached),
        output: field(usage, "completion_tokens"),
        cache_read: cached,
        cache_creation: 0,
        reasoning,
    }
}

/// OpenAI responses: `input_tokens` INCLUDES cached → subtract.
fn openai_responses_usage(usage: &Value) -> NormalizedUsage {
    let input = field(usage, "input_tokens");
    let cached = usage
        .get("input_tokens_details")
        .map_or(0, |d| field(d, "cached_tokens"));
    let reasoning = usage
        .get("output_tokens_details")
        .map_or(0, |d| field(d, "reasoning_tokens"));
    NormalizedUsage {
        input: input.saturating_sub(cached),
        output: field(usage, "output_tokens"),
        cache_read: cached,
        cache_creation: 0,
        reasoning,
    }
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
    NormalizedUsage {
        input: prompt.saturating_sub(cached),
        output: candidates + thoughts,
        cache_read: cached,
        cache_creation: 0,
        reasoning: thoughts,
    }
}

/// Claude stream: input side from `message_start` (forward scan — it is the
/// first frame), cumulative output from the LAST `message_delta` with usage.
/// If only one side is present, the extracted side still wins (partial beats
/// none); `message_start` already carries an initial `output_tokens`, which
/// stands when no delta arrived.
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
        Some(claude_usage(usage))
    });
    let delta = frames.iter().rev().find_map(|frame| {
        let json = frame_json(frame)?;
        if json.get("type").and_then(Value::as_str) != Some("message_delta") {
            return None;
        }
        let usage = json.get("usage").filter(|u| u.is_object())?;
        Some(claude_usage(usage))
    });
    match (start, delta) {
        (Some(start), Some(delta)) => Some(NormalizedUsage {
            output: delta.output,
            ..start
        }),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
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
        assert_eq!(u.cache_creation, 70);
        assert_eq!(u.total(), 510);
    }

    #[test]
    fn openai_chat_response_cached_and_reasoning() {
        let body = json!({
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 200,
                "prompt_tokens_details": {"cached_tokens": 600},
                "completion_tokens_details": {"reasoning_tokens": 80}
            }
        });
        let u = from_response(Provider::OpenAi, &body).unwrap();
        assert_eq!(u.input, 400); // prompt - cached
        assert_eq!(u.cache_read, 600);
        assert_eq!(u.output, 200);
        assert_eq!(u.reasoning, 80);
        assert_eq!(u.cache_creation, 0);

        // Missing details → cache 0, full input.
        let plain = json!({"usage": {"prompt_tokens": 1000, "completion_tokens": 200}});
        let u = from_response(Provider::OpenAi, &plain).unwrap();
        assert_eq!(u.input, 1000);
        assert_eq!(u.cache_read, 0);
        assert_eq!(u.reasoning, 0);
    }

    #[test]
    fn gemini_response_thoughts_and_cached() {
        let body = json!({
            "usageMetadata": {
                "promptTokenCount": 500,
                "candidatesTokenCount": 100,
                "cachedContentTokenCount": 200,
                "thoughtsTokenCount": 30,
                "totalTokenCount": 630
            }
        });
        let u = from_response(Provider::Gemini, &body).unwrap();
        assert_eq!(u.input, 300); // prompt - cached
        assert_eq!(u.output, 130); // candidates + thoughts (thinking billed as output)
        assert_eq!(u.reasoning, 30);
        assert_eq!(u.cache_read, 200);
    }

    #[test]
    fn stream_frames_final_usage() {
        // Claude: message_start input + cumulative message_delta output (last wins).
        let frames = vec![
            SseFrame::event(
                "message_start",
                json!({"type": "message_start", "message": {"usage": {
                    "input_tokens": 25, "output_tokens": 1,
                    "cache_read_input_tokens": 10
                }}})
                .to_string(),
            ),
            SseFrame::event(
                "message_delta",
                json!({"type": "message_delta", "usage": {"output_tokens": 5}}).to_string(),
            ),
            SseFrame::event(
                "message_delta",
                json!({"type": "message_delta", "usage": {"output_tokens": 12}}).to_string(),
            ),
        ];
        let u = from_stream_frames(ContentGenerationKind::ClaudeMessages, &frames).unwrap();
        assert_eq!(u.input, 25);
        assert_eq!(u.cache_read, 10);
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
