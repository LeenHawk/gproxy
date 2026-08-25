//! Downstream response decoration for the OpenRouter-compatible `usage.cost`
//! extension. Decoration happens after protocol conversion.

use bytes::Bytes;
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

use super::Settlement;
use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind};
use crate::usage::NormalizedUsage;

/// Add the authoritative local USD cost to a successful buffered JSON body.
/// Non-JSON responses (audio/video/file bytes and text subtitle formats) are
/// returned byte-for-byte unchanged.
pub fn inject_full(body: Bytes, op: OperationKey, settlement: &Settlement) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let key = match op.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            "usageMetadata"
        }
        _ => "usage",
    };
    let Some(root) = value.as_object_mut() else {
        return body;
    };
    let usage = root
        .entry(key)
        .or_insert_with(|| synthesized_usage(op, &settlement.usage));
    if !usage.is_object() {
        *usage = synthesized_usage(op, &settlement.usage);
    }
    let Some(usage) = usage.as_object_mut() else {
        return body;
    };
    usage.insert("cost".into(), decimal_number(settlement.cost));
    serde_json::to_vec(&value).map(Bytes::from).unwrap_or(body)
}

pub fn inject_buffered_stream(body: Bytes, op: OperationKey, settlement: &Settlement) -> Bytes {
    let Ok(mut frames) = super::frames::decode(&body) else {
        return body;
    };
    if frames.is_empty() {
        return body;
    }
    decorate_terminal(&mut frames, op, settlement);
    Bytes::from(encode_frames(frames))
}

/// Decorate the terminal event of a downstream SSE stream. One complete frame
/// is retained as look-behind so a trailing `[DONE]`/`message_stop` never races
/// ahead of settlement publication.
pub fn inject_stream(
    stream: crate::pipeline::outcome::ByteStream,
    op: OperationKey,
    signal: super::SettlementSignal,
) -> crate::pipeline::outcome::ByteStream {
    use crate::transform::common::sse::{SseDecoder, SseFrame};
    use futures_util::StreamExt;

    struct State {
        inner: Option<crate::pipeline::outcome::ByteStream>,
        decoder: SseDecoder,
        pending: Option<SseFrame>,
        held: Vec<SseFrame>,
        holding: bool,
        flushed: bool,
        op: OperationKey,
        signal: super::SettlementSignal,
    }

    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(stream),
            decoder: SseDecoder::with_limits(crate::transform::common::sse::SseLimits {
                max_frame_bytes: 8 * 1024 * 1024,
                max_buffer_bytes: 16 * 1024 * 1024,
            }),
            pending: None,
            held: Vec::new(),
            holding: false,
            flushed: false,
            op,
            signal,
        },
        |mut state| async move {
            loop {
                if state.inner.is_none() {
                    if state.flushed {
                        return None;
                    }
                    state.flushed = true;
                    if let Ok(Some(frame)) = state.decoder.finish() {
                        if let Some(pending) = state.pending.take() {
                            state.held.push(pending);
                        }
                        state.held.push(frame);
                    }
                    if let Some(frame) = state.pending.take() {
                        state.held.push(frame);
                    }
                    if let Some(settlement) = state.signal.get() {
                        decorate_terminal(&mut state.held, state.op, &settlement);
                    }
                    let bytes = encode_frames(std::mem::take(&mut state.held));
                    if bytes.is_empty() {
                        return None;
                    }
                    return Some((Ok(Bytes::from(bytes)), state));
                }

                let next = state.inner.as_mut().expect("checked").next().await;
                match next {
                    Some(Ok(chunk)) => {
                        let frames = match state.decoder.push(&chunk) {
                            Ok(frames) => frames,
                            Err(error) => {
                                state.inner = None;
                                return Some((
                                    Err(crate::http::client::ClientError::Decode(
                                        error.to_string(),
                                    )),
                                    state,
                                ));
                            }
                        };
                        let mut ready = Vec::new();
                        for frame in frames {
                            if state.holding || terminal_frame(state.op, &frame) {
                                if !state.holding {
                                    state.holding = true;
                                    if let Some(pending) = state.pending.take() {
                                        if usage_frame(state.op, &pending) {
                                            state.held.push(pending);
                                        } else {
                                            ready.push(pending);
                                        }
                                    }
                                }
                                state.held.push(frame);
                            } else if let Some(previous) = state.pending.replace(frame) {
                                ready.push(previous);
                            }
                        }
                        let bytes = encode_frames(ready);
                        if !bytes.is_empty() {
                            return Some((Ok(Bytes::from(bytes)), state));
                        }
                    }
                    Some(Err(error)) => {
                        state.inner = None;
                        return Some((Err(error), state));
                    }
                    None => state.inner = None,
                }
            }
        },
    ))
}

fn usage_frame(op: OperationKey, frame: &crate::transform::common::sse::SseFrame) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(&frame.data) else {
        return false;
    };
    match op.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            value.get("usageMetadata").is_some_and(Value::is_object)
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => value
            .pointer("/response/usage")
            .is_some_and(Value::is_object),
        _ => value.get("usage").is_some_and(Value::is_object),
    }
}

fn terminal_frame(op: OperationKey, frame: &crate::transform::common::sse::SseFrame) -> bool {
    if frame.data == "[DONE]" {
        return true;
    }
    let Ok(value) = serde_json::from_str::<Value>(&frame.data) else {
        return false;
    };
    match op.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChatCompletions) => {
            // Converted Claude streams carry usage on message_start; [DONE] is terminal.
            false
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => value.get("type").and_then(Value::as_str) == Some("response.completed"),
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            value.get("type").and_then(Value::as_str) == Some("message_stop")
        }
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => value
            .pointer("/candidates/0/finishReason")
            .is_some_and(|reason| !reason.is_null()),
        OperationKind::Provider(_) => value.get("usage").is_some_and(Value::is_object),
        _ => false,
    }
}

fn decorate_terminal(
    frames: &mut Vec<crate::transform::common::sse::SseFrame>,
    op: OperationKey,
    settlement: &Settlement,
) {
    for frame in frames.iter_mut().rev() {
        if frame.data == "[DONE]" {
            continue;
        }
        let Ok(mut value) = serde_json::from_str::<Value>(&frame.data) else {
            continue;
        };
        if inject_value(&mut value, op, settlement) {
            frame.data = value.to_string();
            return;
        }
    }

    let synthetic = synthetic_cost_frame(op, settlement);
    let index = frames
        .iter()
        .position(|frame| frame.data == "[DONE]" || is_stop_frame(frame))
        .unwrap_or(frames.len());
    frames.insert(index, synthetic);
}

fn inject_value(value: &mut Value, op: OperationKey, settlement: &Settlement) -> bool {
    if matches!(
        op.kind(),
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponses
                | ContentGenerationKind::OpenAiResponsesWebSocket
        )
    ) && value.get("type").and_then(Value::as_str) == Some("response.completed")
    {
        let Some(response) = value.get_mut("response").and_then(Value::as_object_mut) else {
            return false;
        };
        let usage = response
            .entry("usage")
            .or_insert_with(|| synthesized_usage(op, &settlement.usage));
        if !usage.is_object() {
            *usage = synthesized_usage(op, &settlement.usage);
        }
        usage
            .as_object_mut()
            .expect("usage object")
            .insert("cost".into(), decimal_number(settlement.cost));
        return true;
    }

    let key = match op.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            "usageMetadata"
        }
        _ => "usage",
    };
    let Some(root) = value.as_object_mut() else {
        return false;
    };
    let Some(usage) = root.get_mut(key).filter(|usage| usage.is_object()) else {
        return false;
    };
    usage
        .as_object_mut()
        .expect("usage object")
        .insert("cost".into(), decimal_number(settlement.cost));
    true
}

fn synthetic_cost_frame(
    op: OperationKey,
    settlement: &Settlement,
) -> crate::transform::common::sse::SseFrame {
    use crate::transform::common::sse::SseFrame;
    let mut usage = synthesized_usage(op, &settlement.usage);
    usage
        .as_object_mut()
        .expect("synthesized usage object")
        .insert("cost".into(), decimal_number(settlement.cost));
    match op.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChatCompletions) => {
            SseFrame::data(json!({"choices": [], "usage": usage}).to_string())
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => SseFrame::event(
            "response.completed",
            json!({"type": "response.completed", "response": {"usage": usage}}).to_string(),
        ),
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => SseFrame::event(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {"stop_reason": null, "stop_sequence": null},
                "usage": usage,
            })
            .to_string(),
        ),
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            SseFrame::data(json!({"usageMetadata": usage}).to_string())
        }
        _ => SseFrame::data(json!({"usage": usage}).to_string()),
    }
}

fn is_stop_frame(frame: &crate::transform::common::sse::SseFrame) -> bool {
    serde_json::from_str::<Value>(&frame.data)
        .ok()
        .and_then(|value| value.get("type")?.as_str().map(str::to_owned))
        .is_some_and(|kind| kind == "message_stop")
}

fn encode_frames(frames: Vec<crate::transform::common::sse::SseFrame>) -> Vec<u8> {
    let mut output = String::new();
    for frame in frames {
        output.push_str(&frame.encode());
    }
    output.into_bytes()
}

fn synthesized_usage(op: OperationKey, usage: &NormalizedUsage) -> Value {
    match op.kind() {
        OperationKind::ContentGeneration(kind) => match kind {
            ContentGenerationKind::OpenAiChatCompletions => openai_chat_usage(usage),
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket => openai_responses_usage(usage),
            ContentGenerationKind::ClaudeMessages => claude_usage(usage),
            ContentGenerationKind::GeminiGenerateContent => gemini_usage(usage),
            _ => unreachable!(
                "new non-exhaustive content kind requires usage.cost response placement"
            ),
        },
        OperationKind::Provider(_) => match op.operation() {
            Operation::CreateEmbedding | Operation::Rerank => json!({
                "prompt_tokens": prompt_tokens(usage),
                "total_tokens": usage.total(),
            }),
            Operation::CreateImage | Operation::EditImage => json!({
                "input_tokens": prompt_tokens(usage),
                "output_tokens": usage.output + usage.image_output,
                "total_tokens": usage.total(),
            }),
            Operation::CreateTranscription | Operation::CreateTranslation => json!({
                "type": "tokens",
                "input_tokens": usage.input,
                "output_tokens": usage.output,
                "total_tokens": usage.total(),
            }),
            Operation::CompactContent => openai_responses_usage(usage),
            Operation::WebSearch | Operation::CreateSpeech => openai_responses_usage(usage),
            Operation::RetrieveVideo => Value::Object(Map::new()),
            _ => Value::Object(Map::new()),
        },
        _ => unreachable!("new non-exhaustive operation kind requires usage.cost placement"),
    }
}

fn prompt_tokens(usage: &NormalizedUsage) -> u64 {
    usage.input + usage.cache_read + usage.cache_creation()
}

fn openai_chat_usage(usage: &NormalizedUsage) -> Value {
    json!({
        "prompt_tokens": prompt_tokens(usage),
        "completion_tokens": usage.output + usage.image_output,
        "total_tokens": usage.total(),
        "prompt_tokens_details": {
            "cached_tokens": usage.cache_read,
            "cache_write_tokens": usage.cache_creation(),
        },
        "completion_tokens_details": {
            "reasoning_tokens": usage.reasoning,
            "image_tokens": usage.image_output,
        },
    })
}

fn openai_responses_usage(usage: &NormalizedUsage) -> Value {
    json!({
        "input_tokens": prompt_tokens(usage),
        "output_tokens": usage.output + usage.image_output,
        "total_tokens": usage.total(),
        "input_tokens_details": {
            "cached_tokens": usage.cache_read,
            "cache_write_tokens": usage.cache_creation(),
        },
        "output_tokens_details": {
            "reasoning_tokens": usage.reasoning,
            "image_tokens": usage.image_output,
        },
    })
}

fn claude_usage(usage: &NormalizedUsage) -> Value {
    json!({
        "input_tokens": usage.input,
        "output_tokens": usage.output + usage.image_output,
        "cache_read_input_tokens": usage.cache_read,
        "cache_creation_input_tokens": usage.cache_creation(),
        "cache_creation": {
            "ephemeral_5m_input_tokens": usage.cache_creation_5m,
            "ephemeral_1h_input_tokens": usage.cache_creation_1h,
        },
    })
}

fn gemini_usage(usage: &NormalizedUsage) -> Value {
    json!({
        "promptTokenCount": prompt_tokens(usage),
        "candidatesTokenCount": usage.output + usage.image_output,
        "cachedContentTokenCount": usage.cache_read,
        "thoughtsTokenCount": usage.reasoning,
        "totalTokenCount": usage.total(),
    })
}

fn decimal_number(value: Decimal) -> Value {
    serde_json::from_str(&value.normalize().to_string()).unwrap_or_else(|_| Value::from(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{OperationKey, Provider};
    use crate::usage::{Ended, UsageSource};

    fn settlement() -> Settlement {
        Settlement {
            usage: NormalizedUsage {
                input: 10,
                output: 5,
                cache_read: 2,
                ..Default::default()
            },
            cost: "0.000123".parse().unwrap(),
            source: UsageSource::Upstream,
            ended: Ended::Complete,
        }
    }

    #[test]
    fn injects_number_without_replacing_existing_usage() {
        let op = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        );
        let out = inject_full(
            Bytes::from_static(
                br#"{"usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}"#,
            ),
            op,
            &settlement(),
        );
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["usage"]["prompt_tokens"], 12);
        assert_eq!(value["usage"]["cost"], json!(0.000123));
    }

    #[test]
    fn synthesizes_usage_but_keeps_non_json_unchanged() {
        let op = OperationKey::provider(Operation::CreateEmbedding, Provider::OpenAi);
        let out = inject_full(Bytes::from_static(br#"{"data":[]}"#), op, &settlement());
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["usage"]["total_tokens"], 17);
        assert_eq!(value["usage"]["cost"], json!(0.000123));

        let raw = Bytes::from_static(b"ID3 audio");
        assert_eq!(inject_full(raw.clone(), op, &settlement()), raw);
    }

    #[tokio::test]
    async fn relays_early_chat_usage_and_decorates_terminal_before_done() {
        use futures_util::StreamExt;
        let op = OperationKey::content_generation(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        );
        let signal = super::super::SettlementSignal::default();
        signal.publish(settlement());
        let source: crate::pipeline::outcome::ByteStream = Box::pin(
            futures_util::stream::iter(vec![
                Ok(Bytes::from_static(
                    br#"data: {"choices":[{"delta":{"role":"assistant"}}],"usage":{"prompt_tokens":12,"completion_tokens":0}}

"#,
                )),
                Ok(Bytes::from_static(
                    br#"data: {"choices":[{"delta":{"content":"hi"}}]}

"#,
                )),
                Ok(Bytes::from_static(
                    br#"data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}

data: [DONE]

"#,
                )),
            ]),
        );
        let chunks: Vec<Bytes> = inject_stream(source, op, signal)
            .map(|item| item.unwrap())
            .collect()
            .await;
        let first = String::from_utf8_lossy(&chunks[0]);
        assert!(first.contains(r#""role":"assistant""#), "{first}");
        assert!(!first.contains(r#""content":"hi""#), "{first}");
        assert!(!first.contains("[DONE]"), "{first}");
        let text = String::from_utf8(chunks.concat()).unwrap();
        assert!(text.contains("\"cost\":0.000123"));
        assert!(text.find("\"cost\"").unwrap() < text.find("[DONE]").unwrap());
    }

}
