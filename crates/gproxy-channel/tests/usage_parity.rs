//! Old channel usage readers against the standard readers in
//! `gproxy_protocol::usage`, fixture by fixture.
//!
//! Usage is moving out of the channels. Today each channel reads it from the
//! upstream's raw bytes through its own `UsageExtractor` or `UsageStream`;
//! later it will be read once, by operation and dialect, from the standard
//! response the channel has shaped. This file is the safety net for that
//! move. Each test feeds one existing channel fixture to both paths and
//! asserts that they agree. Where they are meant to differ, the test applies
//! the difference to the old reading before comparing and says why, so every
//! change in what gets billed is written down here rather than discovered on
//! an invoice.
//!
//! Intended differences, each covered below:
//!
//! - **Cache writes.** Every OpenAI-shaped reading follows `openai_wire`: a
//!   `cache_write_tokens` detail leaves the ordinary input and is carried in
//!   the 30-minute bucket. `shared::compatible` and `shared::vendor_usage`
//!   left it inside `input_tokens`.
//! - **Modality metrics.** Audio, text and image detail counts are recorded
//!   as metrics flagged `token_modalities_in_totals`, so pricing takes them
//!   out of the totals instead of charging them twice. `openai_wire` never
//!   set the flag, and for images it took the image tokens out of
//!   `output_tokens` itself; AI Studio did the same for Gemini's image and
//!   audio output.
//! - **Absent is not zero, zero is not absent.** Gemini leaves out counts
//!   that are zero, so a settled reply reports a zero cache read explicitly;
//!   a reasoning count stays unknown when Gemini does not report one.
//! - **Attempts.** A Claude reply becomes attempts only when it names more
//!   than one model (a server-side fallback) or was refused. The
//!   `vendor_usage` and `aws_bedrock` readers also wrapped every ordinary
//!   answer in one attempt naming the reply's model; the exchange's own
//!   model prices it the same way.
//! - **Fields the old reader dropped.** `compatible` and `vendor_usage`
//!   ignored Claude's server tools, speed, tier and geo, OpenAI's audio
//!   details and web searches, and the serving tier; the standard readers
//!   keep them.
//! - **Vendor extras.** OpenRouter's cost, byok flag and serving model, xAI
//!   and Grok Build's cost ticks, stated dollars, video seconds and
//!   non-standard web-search field, DeepSeek's `prompt_cache_hit_tokens`,
//!   Kimi's top-level `cached_tokens`, DashScope's image counters and AI
//!   Studio's service-tier header are not standard fields. Phase 4 moves
//!   them to a per-channel `usage_extras` hook; here the test lists what the
//!   standard reading is missing.
//!
//! Not compared, because their readings today are broken rather than
//! different — the reader sees the upstream's bytes before the channel shapes
//! them — and phase 4 fixes them by reading the shaped response: `kiro`
//! (AWS event stream), `devin` (Connect framing and protobuf), `claudeweb`
//! (claude.ai's own events, character estimates only), and the image replies
//! of `workbuddy` and `dashscope` (the vendor envelope rather than the
//! unwrapped OpenAI reply). `aws_bedrock`, `geminicli`, `antigravity` and
//! `cline` do read correctly today by undoing their framing inside the
//! reader; they are compared against the standard reading of the shaped
//! response.

use gproxy_channel::{
    BaseChannel,
    channel::{
        NormalizedUsage, ResponseView, UsageCompleteness, UsageContext, UsageFrame,
        UsageStreamContext, UsageStreamEnd, UsageTransport,
    },
};
use gproxy_protocol::{
    Dialect, Operation, OperationKey,
    connection::StreamFraming,
    usage::{self, UsageReader},
};
use http::{HeaderMap, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};

// ------------------------------------------------------------------ harness

const SSE: UsageTransport = UsageTransport::Http {
    framing: Some(StreamFraming::Sse),
};

/// What the channel's extractor reads out of a whole body today.
fn old_whole<C: BaseChannel>(
    channel: &C,
    operation: Operation,
    dialect: Dialect,
    body: &[u8],
) -> Option<NormalizedUsage> {
    let headers = HeaderMap::new();
    channel
        .usage_extractor()
        .expect("the channel meters whole bodies")
        .extract(UsageContext {
            operation: OperationKey { operation, dialect },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body,
            },
        })
        .expect("the old reader does not fail")
}

/// What the channel's stream observer reads today.
fn old_stream<C: BaseChannel>(
    channel: &C,
    operation: Operation,
    dialect: Dialect,
    transport: UsageTransport,
    wire: &[u8],
) -> Option<NormalizedUsage> {
    let headers = HeaderMap::new();
    let mut observer = channel
        .usage_stream()
        .expect("the channel watches streams")
        .start(UsageStreamContext {
            operation: OperationKey { operation, dialect },
            request_body: None,
            status: StatusCode::OK,
            headers: &headers,
            transport,
        })
        .expect("an observer");
    for chunk in wire.chunks(7) {
        observer.observe(UsageFrame::HttpChunk(chunk)).unwrap();
    }
    observer.finish(UsageStreamEnd::Complete).unwrap()
}

fn new_whole(operation: Operation, dialect: Dialect, body: &[u8]) -> Option<NormalizedUsage> {
    usage::whole(operation, dialect, body)
}

/// The standard stream reading, which must not depend on how the stream is
/// chunked; every split is checked against the unsplit reading.
fn new_stream(
    operation: Operation,
    dialect: Dialect,
    transport: UsageTransport,
    wire: &[u8],
) -> Option<NormalizedUsage> {
    let read = |size: usize| {
        let mut reader = UsageReader::new(operation, dialect, transport).expect("watchable");
        for chunk in wire.chunks(size.max(1)) {
            reader.push(chunk);
        }
        reader.finish(UsageStreamEnd::Complete)
    };
    let whole = read(wire.len());
    for size in [1, 3, 7, 64] {
        assert_eq!(read(size), whole, "split {size}");
    }
    whole
}

fn sse(events: &[Value]) -> Vec<u8> {
    events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>()
        .into_bytes()
}

fn body(value: &Value) -> Vec<u8> {
    value.to_string().into_bytes()
}

fn decimal(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// The standard reading marks modality metrics as subsets of the totals.
fn flag_modalities(usage: &mut NormalizedUsage) {
    usage
        .dimensions
        .insert("token_modalities_in_totals".into(), "true".into());
}

/// `vendor_usage` and `aws_bedrock` wrap an ordinary Claude answer in one
/// attempt naming its model; the standard reading leaves the exchange's
/// model to price it.
fn drop_single_attempt(usage: &mut NormalizedUsage) {
    assert_eq!(usage.attempts.len(), 1);
    assert_eq!(usage.attempts[0].billable, Some(true));
    usage.attempts.clear();
}

// ------------------------------------------------------------- openai

#[cfg(feature = "openai")]
mod openai {
    use super::*;
    use gproxy_channel::channels::openai::OpenAi;

    #[test]
    fn chat_and_responses_bodies_are_equal_but_modalities_are_flagged() {
        let chat = body(&json!({"service_tier": "flex", "usage": {
            "prompt_tokens": 100, "completion_tokens": 20,
            "prompt_tokens_details": {"cached_tokens": 40, "cache_write_tokens": 3, "audio_tokens": 5},
            "completion_tokens_details": {"reasoning_tokens": 7}}}));
        let mut old = old_whole(
            &OpenAi,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            &chat,
        );
        flag_modalities(old.as_mut().unwrap());
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, &chat),
            old
        );

        let responses = body(&json!({"usage": {"input_tokens": 11, "output_tokens": 4,
            "input_tokens_details": {"cached_tokens": 2},
            "output_tokens_details": {"reasoning_tokens": 1},
            "server_tool_use": {"web_search_requests": 3}}}));
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAi, &responses),
            old_whole(
                &OpenAi,
                Operation::GenerateContent,
                Dialect::OpenAi,
                &responses
            )
        );
        let empty = br#"{"id":"resp_1"}"#;
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAi, empty),
            None
        );
        assert_eq!(
            old_whole(&OpenAi, Operation::GenerateContent, Dialect::OpenAi, empty),
            None
        );
    }

    /// `openai_wire` took the image tokens out of `output_tokens`; the
    /// standard reading keeps the total whole and flags the image metric as
    /// a subset of it, which prices the same whenever image output has a
    /// price of its own.
    #[test]
    fn an_image_reply_keeps_its_output_total_whole() {
        let reply = br#"{"data":[{"b64_json":"x"}],"usage":{"input_tokens":2,"output_tokens":8}}"#;
        let mut old = old_whole(&OpenAi, Operation::CreateImage, Dialect::OpenAi, reply).unwrap();
        assert_eq!(old.tokens.output_tokens, Some(0));
        old.tokens.output_tokens = Some(8);
        flag_modalities(&mut old);
        assert_eq!(
            new_whole(Operation::CreateImage, Dialect::OpenAi, reply),
            Some(old)
        );
    }

    #[test]
    fn a_transcription_is_equal() {
        for reply in [
            &br#"{"text":"hi","usage":{"type":"tokens","input_tokens":14,"output_tokens":45}}"#[..],
            br#"{"text":"hi","usage":{"type":"duration","seconds":3.5}}"#,
        ] {
            assert_eq!(
                new_whole(Operation::CreateTranscription, Dialect::OpenAi, reply),
                old_whole(
                    &OpenAi,
                    Operation::CreateTranscription,
                    Dialect::OpenAi,
                    reply
                )
            );
        }
    }

    #[test]
    fn chat_and_responses_streams_are_equal() {
        let chat = [
            &b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n"[..],
            b"data: {\"usage\":{\"prompt_tokens\":9,\"completion_",
            b"tokens\":4}}\n\ndata: [DONE]\n\n",
        ]
        .concat();
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                SSE,
                &chat
            ),
            old_stream(
                &OpenAi,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                SSE,
                &chat
            )
        );
        let responses = [
            &b"event: response.in_progress\ndata: {\"type\":\"response.in_progress\"}\n\n"[..],
            b"event: response.com",
            b"pleted\r\ndata: {\"type\":\"response.completed\",\"response\":{\"service_tier\":\"priority\",\"usage\":{",
            b"\"input_tokens\":9,\"output_tokens\":4}}}\r\n\r\n",
        ]
        .concat();
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                SSE,
                &responses
            ),
            old_stream(
                &OpenAi,
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                SSE,
                &responses
            )
        );
    }
}

// ---------------------------------------------------------- claudeapi

#[cfg(feature = "claudeapi")]
mod claudeapi {
    use super::*;
    use gproxy_channel::channels::claudeapi::Claudeapi;

    #[test]
    fn a_messages_body_and_the_compatibility_layer_are_equal() {
        let messages = body(&json!({"model": "claude-fable-5", "usage": {
            "input_tokens": 100, "output_tokens": 50, "cache_read_input_tokens": 40,
            "cache_creation": {"ephemeral_5m_input_tokens": 7, "ephemeral_1h_input_tokens": 3},
            "output_tokens_details": {"thinking_tokens": 12},
            "server_tool_use": {"web_search_requests": 2},
            "service_tier": "standard"}}));
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::Claude, &messages),
            old_whole(
                &Claudeapi,
                Operation::GenerateContent,
                Dialect::Claude,
                &messages
            )
        );
        let chat = br#"{"usage":{"prompt_tokens":11,"completion_tokens":4,"prompt_tokens_details":{"cached_tokens":5}}}"#;
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, chat),
            old_whole(
                &Claudeapi,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                chat
            )
        );
    }

    #[test]
    fn a_messages_stream_is_equal() {
        let wire = [
            &b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"in"[..],
            b"put_tokens\":30,\"cache_read_input_tokens\":4,\"output_tokens\":1}}}\n\n",
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":19,\"service_tier\":\"priority\"}}\n\n",
        ]
        .concat();
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::Claude,
                SSE,
                &wire
            ),
            old_stream(
                &Claudeapi,
                Operation::StreamGenerateContent,
                Dialect::Claude,
                SSE,
                &wire
            )
        );
    }
}

// --------------------------------------------------------- claudecode

#[cfg(feature = "claudecode")]
mod claudecode {
    use super::*;
    use gproxy_channel::channels::claudecode::Claudecode;

    #[test]
    fn a_fallback_stream_and_a_buffered_reply_are_equal() {
        let wire = concat!(
            "event: message_start\r\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-fable-5\",\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_read_input_tokens\":10,\"cache_creation\":{\"ephemeral_5m_input_tokens\":0,\"ephemeral_1h_input_tokens\":20}}}}\r\n\r\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"fallback\",\"from\":{\"model\":\"claude-fable-5\"},\"to\":{\"model\":\"claude-opus-4-8\"}}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12,\"cache_creation_input_tokens\":20,\"output_tokens_details\":{\"thinking_tokens\":4},\"iterations\":[{\"type\":\"fallback_message\",\"model\":\"claude-fable-5\",\"input_tokens\":25,\"output_tokens\":0},{\"type\":\"message\",\"input_tokens\":25,\"output_tokens\":12}]}}\n\n"
        )
        .as_bytes();
        let new = new_stream(Operation::StreamGenerateContent, Dialect::Claude, SSE, wire);
        assert_eq!(new.as_ref().unwrap().attempts.len(), 2);
        assert_eq!(
            new,
            old_stream(
                &Claudecode,
                Operation::StreamGenerateContent,
                Dialect::Claude,
                SSE,
                wire
            )
        );

        let reply = body(
            &json!({"model": "claude-sonnet-4-6", "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 4, "cache_read_input_tokens": 30,
                "cache_creation": {"ephemeral_5m_input_tokens": 2, "ephemeral_1h_input_tokens": 3},
                "output_tokens_details": {"thinking_tokens": 1},
                "server_tool_use": {"web_search_requests": 2, "web_fetch_requests": 1},
                "service_tier": "standard", "speed": "fast"}}),
        );
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::Claude, &reply),
            old_whole(
                &Claudecode,
                Operation::GenerateContent,
                Dialect::Claude,
                &reply
            )
        );
    }

    /// The claudecode reader dropped a refusal on the floor; a refused
    /// answer is now one attempt, billable only if it produced output — the
    /// rule `vendor_usage` and `aws_bedrock` already applied.
    #[test]
    fn a_refusal_now_becomes_an_unbillable_attempt() {
        let reply = br#"{"model":"claude-x","stop_reason":"refusal","usage":{"input_tokens":9,"output_tokens":0}}"#;
        let old = old_whole(
            &Claudecode,
            Operation::GenerateContent,
            Dialect::Claude,
            reply,
        )
        .unwrap();
        assert!(old.attempts.is_empty());
        let new = new_whole(Operation::GenerateContent, Dialect::Claude, reply).unwrap();
        assert_eq!(new.attempts.len(), 1);
        assert_eq!(new.attempts[0].billable, Some(false));
        assert_eq!(new.tokens, old.tokens);
    }
}

// ------------------------------------------------------------ aistudio

#[cfg(feature = "aistudio")]
mod aistudio {
    use super::*;
    use gproxy_channel::channels::aistudio::Aistudio;

    /// AI Studio took image and audio tokens out of the output and dropped a
    /// zero reasoning count; the standard reading keeps the output whole,
    /// flags the media metrics as subsets and reports the tool-use prompt.
    #[test]
    fn a_reply_keeps_media_inside_the_output() {
        let reply = body(&json!({"usageMetadata": {"promptTokenCount": 100,
            "cachedContentTokenCount": 30, "candidatesTokenCount": 50, "thoughtsTokenCount": 20,
            "toolUsePromptTokenCount": 4,
            "candidatesTokensDetails": [{"modality": "TEXT", "tokenCount": 10},
                {"modality": "IMAGE", "tokenCount": 40}], "serviceTier": "flex"}}));
        let mut old = old_whole(
            &Aistudio,
            Operation::GenerateContent,
            Dialect::Gemini,
            &reply,
        )
        .unwrap();
        assert_eq!(old.tokens.output_tokens, Some(30));
        old.tokens.output_tokens = Some(70);
        flag_modalities(&mut old);
        old.metrics
            .insert("tool_use_prompt_tokens".into(), 4.into());
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::Gemini, &reply),
            Some(old)
        );
    }

    #[test]
    fn an_embedding_is_equal() {
        let reply = br#"{"embedding":{"values":[0.1]},"usageMetadata":{"promptTokenCount":6}}"#;
        assert_eq!(
            new_whole(Operation::CreateEmbedding, Dialect::Gemini, reply),
            old_whole(
                &Aistudio,
                Operation::CreateEmbedding,
                Dialect::Gemini,
                reply
            )
        );
    }

    /// AI Studio ignored a record without a candidate count, so a stream cut
    /// after it read nothing; the standard reading reports the prompt with
    /// the output left unknown for the estimator. A stream that ends on its
    /// own reads the same either way.
    #[test]
    fn streams_are_equal_once_settled() {
        let partial = r#"{"usageMetadata":{"promptTokenCount":100}}"#;
        let last = r#"{"usageMetadata":{"promptTokenCount":100,"candidatesTokenCount":50,"thoughtsTokenCount":10}}"#;
        let wire = format!("data: {partial}\n\ndata: {last}\n\n");
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                SSE,
                wire.as_bytes()
            ),
            old_stream(
                &Aistudio,
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                SSE,
                wire.as_bytes()
            )
        );
        let array = UsageTransport::Http {
            framing: Some(StreamFraming::JsonArray),
        };
        let wire = format!("[{partial},{last}]");
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                array,
                wire.as_bytes()
            ),
            old_stream(
                &Aistudio,
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                array,
                wire.as_bytes()
            )
        );

        let cut = format!("data: {partial}\n\n");
        assert_eq!(
            old_stream(
                &Aistudio,
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                SSE,
                cut.as_bytes()
            ),
            None
        );
        let mut reader =
            UsageReader::new(Operation::StreamGenerateContent, Dialect::Gemini, SSE).unwrap();
        reader.push(cut.as_bytes());
        let new = reader.finish(UsageStreamEnd::Interrupted).unwrap();
        assert_eq!(new.tokens.input_tokens, Some(100));
        assert_eq!(new.tokens.output_tokens, None);
        assert_eq!(new.completeness, UsageCompleteness::Partial);
    }

    /// Extra: AI Studio names the tier it served in the
    /// `x-gemini-service-tier` header, which no body reader sees.
    #[test]
    fn extra_the_service_tier_header_is_not_a_body_field() {
        let reply = br#"{"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1}}"#;
        let mut headers = HeaderMap::new();
        headers.insert("x-gemini-service-tier", "flex".parse().unwrap());
        let old = Aistudio
            .usage_extractor()
            .unwrap()
            .extract(UsageContext {
                operation: OperationKey {
                    operation: Operation::GenerateContent,
                    dialect: Dialect::Gemini,
                },
                request_body: None,
                response: ResponseView {
                    status: StatusCode::OK,
                    headers: &headers,
                    body: reply,
                },
            })
            .unwrap()
            .unwrap();
        assert_eq!(old.actual_service_tier.as_deref(), Some("flex"));
        let new = new_whole(Operation::GenerateContent, Dialect::Gemini, reply).unwrap();
        assert_eq!(new.actual_service_tier, None);
    }
}

// ------------------------------------------------- code assist (gemini)

/// Code Assist wraps each Gemini reply under `response`, and the channel
/// shaping takes the wrapper off; the standard reader sees the unwrapped
/// reply. The shared Code Assist reader reported a zero reasoning count
/// where Gemini reported none, and did not flag its media metrics.
#[cfg(any(feature = "geminicli", feature = "antigravity"))]
fn code_assist_parity<C: BaseChannel>(channel: &C) {
    let inner = json!({"usageMetadata": {"promptTokenCount": 100, "cachedContentTokenCount": 40,
        "candidatesTokenCount": 10, "thoughtsTokenCount": 5,
        "candidatesTokensDetails": [{"modality": "IMAGE", "tokenCount": 4}]}});
    let mut old = old_whole(
        channel,
        Operation::GenerateContent,
        Dialect::Gemini,
        &body(&json!({"response": inner})),
    )
    .unwrap();
    flag_modalities(&mut old);
    assert_eq!(
        new_whole(Operation::GenerateContent, Dialect::Gemini, &body(&inner)),
        Some(old)
    );

    let records = [
        json!({"usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 1}}),
        json!({"usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 7}}),
    ];
    let wrapped: Vec<Value> = records.iter().map(|r| json!({"response": r})).collect();
    let mut old = old_stream(
        channel,
        Operation::StreamGenerateContent,
        Dialect::Gemini,
        SSE,
        &sse(&wrapped),
    )
    .unwrap();
    assert_eq!(old.tokens.reasoning_tokens, Some(0));
    old.tokens.reasoning_tokens = None;
    assert_eq!(
        new_stream(
            Operation::StreamGenerateContent,
            Dialect::Gemini,
            SSE,
            &sse(&records)
        ),
        Some(old)
    );
}

#[cfg(feature = "geminicli")]
#[test]
fn geminicli_reads_the_unwrapped_reply_and_leaves_absent_reasoning_unknown() {
    code_assist_parity(&gproxy_channel::channels::geminicli::GeminiCli);
}

#[cfg(feature = "antigravity")]
#[test]
fn antigravity_reads_the_unwrapped_reply_and_leaves_absent_reasoning_unknown() {
    code_assist_parity(&gproxy_channel::channels::antigravity::Antigravity);
}

// ------------------------------------------------------------------ codex

#[cfg(feature = "codex")]
mod codex {
    use super::*;
    use gproxy_channel::channels::codex::Codex;

    #[test]
    fn responses_bodies_and_streams_are_equal() {
        for details in ["input_tokens_details", "input_token_details"] {
            let usage = json!({"input_tokens": 100, "output_tokens": 20,
                details: {"cached_tokens": 40, "cache_write_tokens": 3}});
            let reply = body(&json!({"usage": usage}));
            assert_eq!(
                new_whole(Operation::GenerateContent, Dialect::OpenAi, &reply),
                old_whole(&Codex, Operation::GenerateContent, Dialect::OpenAi, &reply)
            );
            let wire = sse(&[
                json!({"type": "response.output_text.delta", "delta": "hi"}),
                json!({"type": "response.completed", "response": {"id": "r1", "usage": usage}}),
            ]);
            assert_eq!(
                new_stream(
                    Operation::StreamGenerateContent,
                    Dialect::OpenAi,
                    SSE,
                    &wire
                ),
                old_stream(
                    &Codex,
                    Operation::StreamGenerateContent,
                    Dialect::OpenAi,
                    SSE,
                    &wire
                )
            );
        }
    }

    /// Codex dropped web-search counts; the standard reading keeps them.
    #[test]
    fn web_searches_are_now_kept() {
        let reply = body(&json!({"usage": {"input_tokens": 5, "output_tokens": 1,
            "server_tool_use": {"web_search_requests": 2}}}));
        let mut old =
            old_whole(&Codex, Operation::GenerateContent, Dialect::OpenAi, &reply).unwrap();
        old.metrics.insert("web_searches".into(), 2.into());
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAi, &reply),
            Some(old)
        );
    }

    #[test]
    fn an_image_stream_is_equal() {
        for (operation, kind) in [
            (Operation::CreateImage, "image_generation.completed"),
            (Operation::EditImage, "image_edit.completed"),
        ] {
            let event = json!({"type": kind, "generation_id": "g", "quality": "high",
                "size": "1024x1024", "usage": {"input_tokens": 15, "output_tokens": 10,
                "input_tokens_details": {"image_tokens": 12, "text_tokens": 3},
                "output_tokens_details": {"image_tokens": 10}}});
            let wire = format!("event: {kind}\ndata: {event}\n\n");
            assert_eq!(
                new_stream(operation, Dialect::OpenAi, SSE, wire.as_bytes()),
                old_stream(&Codex, operation, Dialect::OpenAi, SSE, wire.as_bytes())
            );
        }
    }

    #[test]
    fn a_realtime_session_is_equal() {
        let done = json!({"type": "response.done", "response": {"id": "r1", "usage": {
            "input_tokens": 100, "output_tokens": 30,
            "input_token_details": {"cached_tokens": 20, "audio_tokens": 70, "text_tokens": 30,
                "cached_tokens_details": {"audio_tokens": 15, "text_tokens": 5}},
            "output_token_details": {"audio_tokens": 25, "text_tokens": 5}}}})
        .to_string();
        let second = json!({"type": "response.done", "response": {"id": "r2",
            "usage": {"input_tokens": 10, "output_tokens": 4}}})
        .to_string();
        let headers = HeaderMap::new();
        let mut observer = Codex
            .usage_stream()
            .unwrap()
            .start(UsageStreamContext {
                operation: OperationKey {
                    operation: Operation::ConnectRealtime,
                    dialect: Dialect::OpenAi,
                },
                request_body: None,
                status: StatusCode::OK,
                headers: &headers,
                transport: UsageTransport::WebSocket,
            })
            .unwrap();
        let mut reader = UsageReader::new(
            Operation::ConnectRealtime,
            Dialect::OpenAi,
            UsageTransport::WebSocket,
        )
        .unwrap();
        for text in [&done, &done, &second] {
            let frame = gproxy_protocol::connection::WsFrame::Text(text.clone());
            observer.observe(UsageFrame::WebSocket(&frame)).unwrap();
            reader.push_message(text);
        }
        assert_eq!(
            reader.finish(UsageStreamEnd::Complete),
            observer.finish(UsageStreamEnd::Complete).unwrap()
        );
    }
}

// -------------------------------------------- vendor_usage passthroughs

/// Azure, Vertex, Vertex Express, Custom, Vercel, NVIDIA and the Cloudflare
/// AI Gateway read the vendor's own block through `shared::vendor_usage`,
/// streams included — by buffering the whole stream and scanning it.
#[cfg(any(
    feature = "azure",
    feature = "vertex",
    feature = "vertexexpress",
    feature = "custom",
    feature = "vercel",
    feature = "nvidia",
    feature = "cloudflare_ai_gateway"
))]
mod vendor_usage {
    use super::*;

    fn whole<C: BaseChannel>(channel: &C, dialect: Dialect, reply: &[u8]) {
        let old = old_whole(channel, Operation::GenerateContent, dialect, reply);
        assert_eq!(new_whole(Operation::GenerateContent, dialect, reply), old);
    }

    /// The standard reading of a watched stream, which the old path read by
    /// buffering the whole stream and handing it to its extractor.
    fn streamed(dialect: Dialect, wire: &[u8]) -> NormalizedUsage {
        new_stream(Operation::StreamGenerateContent, dialect, SSE, wire).expect("usage")
    }

    const CHAT: &str = r#"{"choices":[{"message":{"content":"hi"}}],"usage":{"prompt_tokens":150,"completion_tokens":40,"prompt_tokens_details":{"cached_tokens":50},"completion_tokens_details":{"reasoning_tokens":12}}}"#;
    const RESPONSES: &str = r#"{"usage":{"input_tokens":120,"output_tokens":30,"input_tokens_details":{"cached_tokens":20},"output_tokens_details":{"reasoning_tokens":10}}}"#;
    const GEMINI: &str = r#"{"usageMetadata":{"promptTokenCount":200,"candidatesTokenCount":60,"thoughtsTokenCount":25,"cachedContentTokenCount":80,"toolUsePromptTokenCount":13}}"#;

    /// OpenAI shapes and a Gemini reply that names every count agree.
    fn plain<C: BaseChannel>(channel: &C, gemini: bool) {
        whole(channel, Dialect::OpenAiChat, CHAT.as_bytes());
        whole(channel, Dialect::OpenAi, RESPONSES.as_bytes());
        if gemini {
            whole(channel, Dialect::Gemini, GEMINI.as_bytes());
        }
    }

    /// `vendor_usage` left a cache write inside the ordinary input and
    /// dropped audio details, web searches and the serving tier.
    fn fields_it_dropped<C: BaseChannel>(channel: &C) {
        let reply = br#"{"service_tier":"priority","usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":40,"cache_write_tokens":3,"audio_tokens":5},"server_tool_use":{"web_search_requests":2}}}"#;
        let mut old = old_whole(
            channel,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            reply,
        )
        .unwrap();
        assert_eq!(old.tokens.input_tokens, Some(60));
        old.tokens.input_tokens = Some(57);
        old.tokens.cache_creation_30m_tokens = Some(3);
        old.metrics.insert("audio_input_tokens".into(), 5.into());
        old.metrics.insert("web_searches".into(), 2.into());
        flag_modalities(&mut old);
        old.dimensions
            .insert("service_tier".into(), "priority".into());
        old.actual_service_tier = Some("priority".into());
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, reply),
            Some(old)
        );
    }

    /// A Claude reply: `vendor_usage` wrapped it in one attempt and ignored
    /// server tools and qualifiers.
    fn claude<C: BaseChannel>(channel: &C) {
        let reply = br#"{"model":"claude-x","stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":9,"cache_read_input_tokens":4,"cache_creation":{"ephemeral_5m_input_tokens":5,"ephemeral_1h_input_tokens":6},"server_tool_use":{"web_search_requests":1,"web_fetch_requests":0},"speed":"fast"}}"#;
        let mut old =
            old_whole(channel, Operation::GenerateContent, Dialect::Claude, reply).unwrap();
        drop_single_attempt(&mut old);
        old.metrics.insert("web_searches".into(), 1.into());
        old.dimensions.insert("speed".into(), "fast".into());
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::Claude, reply),
            Some(old)
        );
        let refused = br#"{"model":"claude-x","stop_reason":"refusal","usage":{"input_tokens":7,"output_tokens":0}}"#;
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::Claude, refused),
            old_whole(
                channel,
                Operation::GenerateContent,
                Dialect::Claude,
                refused
            )
        );
    }

    /// The accumulated streams read the same as the watched ones, except
    /// for the single attempt on an ordinary Claude answer.
    fn streams<C: BaseChannel>(channel: &C, gemini: bool) {
        let claude = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-x\",\"usage\":{\"input_tokens\":11,\"cache_read_input_tokens\":4}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
        );
        let mut old = old_whole(
            channel,
            Operation::StreamGenerateContent,
            Dialect::Claude,
            claude.as_bytes(),
        )
        .unwrap();
        drop_single_attempt(&mut old);
        assert_eq!(streamed(Dialect::Claude, claude.as_bytes()), old);

        let chat = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}],\"usage\":null}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":31,\"completion_tokens\":17,\"prompt_tokens_details\":{\"cached_tokens\":9}}}\n\n",
            "data: [DONE]\n\n",
        );
        let old = old_whole(
            channel,
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            chat.as_bytes(),
        );
        assert_eq!(streamed(Dialect::OpenAiChat, chat.as_bytes()), old.unwrap());

        if gemini {
            let wire = concat!(
                "data: {\"usageMetadata\":{\"promptTokenCount\":40,\"candidatesTokenCount\":2}}\n\n",
                "data: {\"usageMetadata\":{\"promptTokenCount\":40,\"candidatesTokenCount\":18}}\n\n",
            );
            let mut old = old_whole(
                channel,
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                wire.as_bytes(),
            )
            .unwrap();
            // Gemini leaves out a zero cache read; a settled reply states it.
            assert_eq!(old.tokens.cached_input_tokens, None);
            old.tokens.cached_input_tokens = Some(0);
            assert_eq!(streamed(Dialect::Gemini, wire.as_bytes()), old);
        }
    }

    #[cfg(feature = "azure")]
    #[test]
    fn azure_agrees_except_cache_writes_dropped_fields_and_single_attempts() {
        use gproxy_channel::channels::azure::Azure;
        plain(&Azure, false);
        fields_it_dropped(&Azure);
        claude(&Azure);
        streams(&Azure, false);
    }

    #[cfg(feature = "vertex")]
    #[test]
    fn vertex_agrees_except_single_attempts_and_the_explicit_zero_cache() {
        use gproxy_channel::channels::vertex::Vertex;
        whole(&Vertex, Dialect::Gemini, GEMINI.as_bytes());
        claude(&Vertex);
        streams(&Vertex, true);
    }

    #[cfg(feature = "vertexexpress")]
    #[test]
    fn vertexexpress_agrees() {
        use gproxy_channel::channels::vertexexpress::VertexExpress;
        whole(&VertexExpress, Dialect::Gemini, GEMINI.as_bytes());
    }

    #[cfg(feature = "custom")]
    #[test]
    fn custom_agrees_except_cache_writes_dropped_fields_and_single_attempts() {
        use gproxy_channel::channels::custom::Custom;
        plain(&Custom, true);
        fields_it_dropped(&Custom);
        claude(&Custom);
        streams(&Custom, true);
    }

    #[cfg(feature = "vercel")]
    #[test]
    fn vercel_agrees_except_cache_writes_dropped_fields_and_single_attempts() {
        use gproxy_channel::channels::vercel::Vercel;
        plain(&Vercel, true);
        fields_it_dropped(&Vercel);
        claude(&Vercel);
        streams(&Vercel, true);
    }

    #[cfg(feature = "nvidia")]
    #[test]
    fn nvidia_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::nvidia::Nvidia;
        whole(&Nvidia, Dialect::OpenAiChat, CHAT.as_bytes());
        fields_it_dropped(&Nvidia);
    }

    #[cfg(feature = "cloudflare_ai_gateway")]
    #[test]
    fn cloudflare_ai_gateway_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::cloudflare_ai_gateway::CloudflareAiGateway;
        plain(&CloudflareAiGateway, true);
        fields_it_dropped(&CloudflareAiGateway);
    }
}

// ---------------------------------------------- shared::compatible users

/// Copilot CLI, OpenCode, Cline, xAI, Grok Build, OpenRouter, DeepSeek, Kimi
/// and DashScope read through `shared::compatible::usage` and a per-channel
/// hook for their own fields.
#[cfg(any(
    feature = "copilotcli",
    feature = "opencode",
    feature = "cline",
    feature = "xai",
    feature = "grokbuild",
    feature = "openrouter",
    feature = "deepseek",
    feature = "kimi",
    feature = "dashscope"
))]
mod compatible {
    use super::*;

    fn equal<C: BaseChannel>(channel: &C, dialect: Dialect, reply: &Value) {
        let reply = body(reply);
        assert_eq!(
            new_whole(Operation::GenerateContent, dialect, &reply),
            old_whole(channel, Operation::GenerateContent, dialect, &reply)
        );
    }

    /// `compatible` left a cache write inside the ordinary input and
    /// dropped audio details, web searches and the serving tier, as
    /// `vendor_usage` did.
    fn fields_it_dropped<C: BaseChannel>(channel: &C) {
        let reply = br#"{"service_tier":"priority","usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":40,"cache_write_tokens":3,"audio_tokens":5},"server_tool_use":{"web_search_requests":2}}}"#;
        let mut old = old_whole(
            channel,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            reply,
        )
        .unwrap();
        assert_eq!(old.tokens.input_tokens, Some(60));
        old.tokens.input_tokens = Some(57);
        old.tokens.cache_creation_30m_tokens = Some(3);
        old.metrics.insert("audio_input_tokens".into(), 5.into());
        old.metrics.insert("web_searches".into(), 2.into());
        flag_modalities(&mut old);
        old.dimensions
            .insert("service_tier".into(), "priority".into());
        old.actual_service_tier = Some("priority".into());
        let new = new_whole(Operation::GenerateContent, Dialect::OpenAiChat, reply).unwrap();
        // Vendor hooks may have added their own keys; compare the standard part.
        assert_eq!(new.tokens, old.tokens);
        for (key, value) in &new.metrics {
            assert_eq!(old.metrics.get(key), Some(value), "{key}");
        }
        assert_eq!(new.actual_service_tier, old.actual_service_tier);
    }

    /// Chat and Responses streams settle on the same reading.
    fn streams<C: BaseChannel>(channel: &C) {
        let chat = sse(&[
            json!({"choices": [{"delta": {"content": "hi"}}], "usage": null}),
            json!({"choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 2,
                "prompt_tokens_details": {"cached_tokens": 4}}}),
        ]);
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                SSE,
                &chat
            ),
            old_stream(
                channel,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                SSE,
                &chat
            )
        );
        let responses = sse(&[
            json!({"type": "response.output_text.delta", "delta": "hi"}),
            json!({"type": "response.completed", "response": {"usage":
                {"input_tokens": 9, "output_tokens": 3}}}),
        ]);
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                SSE,
                &responses
            ),
            old_stream(
                channel,
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                SSE,
                &responses
            )
        );
    }

    /// A Claude reply: `compatible` ignored server tools and qualifiers.
    fn claude<C: BaseChannel>(channel: &C) {
        let reply = br#"{"usage":{"input_tokens":100,"output_tokens":4,"cache_read_input_tokens":40,"server_tool_use":{"web_search_requests":1},"service_tier":"standard"}}"#;
        let mut old =
            old_whole(channel, Operation::GenerateContent, Dialect::Claude, reply).unwrap();
        old.metrics.insert("web_searches".into(), 1.into());
        old.dimensions
            .insert("service_tier".into(), "standard".into());
        old.actual_service_tier = Some("standard".into());
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::Claude, reply),
            Some(old)
        );
    }

    #[cfg(feature = "copilotcli")]
    #[test]
    fn copilotcli_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::copilotcli::CopilotCli;
        equal(
            &CopilotCli,
            Dialect::OpenAiChat,
            &json!({"usage": {"prompt_tokens": 90,
            "completion_tokens": 7, "prompt_tokens_details": {"cached_tokens": 20}}}),
        );
        fields_it_dropped(&CopilotCli);
        streams(&CopilotCli);
    }

    #[cfg(feature = "opencode")]
    #[test]
    fn opencode_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::opencode::OpenCode;
        equal(
            &OpenCode::ZEN,
            Dialect::OpenAiChat,
            &json!({"usage": {"prompt_tokens": 100,
            "completion_tokens": 4, "prompt_tokens_details": {"cached_tokens": 40}}}),
        );
        equal(
            &OpenCode::ZEN,
            Dialect::Claude,
            &json!({"usage": {"input_tokens": 100,
            "output_tokens": 4, "cache_read_input_tokens": 40}}),
        );
        fields_it_dropped(&OpenCode::ZEN);
        claude(&OpenCode::ZEN);
        streams(&OpenCode::ZEN);
    }

    /// Cline's buffered reply is wrapped in `{"success":true,"data":…}`,
    /// which the channel's shaping takes off before the client sees it.
    #[cfg(feature = "cline")]
    #[test]
    fn cline_reads_the_unwrapped_reply_and_otherwise_agrees() {
        use gproxy_channel::channels::cline::Cline;
        let inner = json!({"usage": {"prompt_tokens": 100, "completion_tokens": 4,
            "prompt_tokens_details": {"cached_tokens": 30}}});
        let old = old_whole(
            &Cline,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            &body(&json!({"success": true, "data": inner})),
        );
        assert_eq!(
            new_whole(
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(&inner)
            ),
            old
        );
        fields_it_dropped(&Cline);
        streams(&Cline);
    }

    /// Extras for phase 4: cost ticks, a stated price, video seconds and the
    /// non-standard `server_side_tool_usage_details` web-search count. The
    /// image input tokens are standard Responses detail and already read.
    fn xai_extras<C: BaseChannel>(channel: &C) {
        let reply = body(&json!({"usage": {"input_tokens": 100, "output_tokens": 5,
            "input_tokens_details": {"cached_tokens": 40, "image_tokens": 12},
            "cost_in_usd_ticks": 1234,
            "server_side_tool_usage_details": {"web_search_requests": 2}}}));
        let mut old =
            old_whole(channel, Operation::GenerateContent, Dialect::OpenAi, &reply).unwrap();
        let new = new_whole(Operation::GenerateContent, Dialect::OpenAi, &reply).unwrap();
        for extra in ["cost_in_usd_ticks", "web_searches"] {
            assert!(old.metrics.remove(extra).is_some(), "{extra}");
        }
        flag_modalities(&mut old);
        assert_eq!(new, old);

        let job = body(&json!({"usage": {"input_tokens": 4, "output_tokens": 0},
            "cost_usd": 0.4, "duration": 6}));
        let mut old =
            old_whole(channel, Operation::GenerateContent, Dialect::OpenAi, &job).unwrap();
        assert_eq!(
            old.metrics.remove("upstream_cost_usd"),
            Some(decimal("0.4"))
        );
        assert_eq!(old.metrics.remove("video_seconds"), Some(6.into()));
        assert_eq!(
            old.dimensions.remove("upstream_priced").as_deref(),
            Some("true")
        );
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAi, &job),
            Some(old)
        );
    }

    #[cfg(feature = "xai")]
    #[test]
    fn xai_agrees_except_cost_ticks_stated_dollars_and_video_seconds() {
        use gproxy_channel::channels::xai::Xai;
        xai_extras(&Xai);
        streams(&Xai);
    }

    #[cfg(feature = "grokbuild")]
    #[test]
    fn grokbuild_agrees_except_cost_ticks_stated_dollars_and_video_seconds() {
        use gproxy_channel::channels::grokbuild::GrokBuild;
        xai_extras(&GrokBuild);
        streams(&GrokBuild);
    }

    /// Extras for phase 4: the charged cost and its breakdown, the byok
    /// flag, the serving model (as a dimension and as the one attempt), and
    /// the video input tokens OpenRouter reports in the Chat details.
    #[cfg(feature = "openrouter")]
    #[test]
    fn openrouter_agrees_except_cost_byok_and_serving_model() {
        use gproxy_channel::channels::openrouter::OpenRouter;
        let reply = body(&json!({"model": "anthropic/claude-opus-4-8", "usage": {
            "prompt_tokens": 100, "completion_tokens": 20,
            "prompt_tokens_details": {"cached_tokens": 30, "video_tokens": 6},
            "cost": 0.0125, "cost_details": {"upstream_inference_cost": 0.011},
            "is_byok": true}}));
        let mut old = old_whole(
            &OpenRouter,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            &reply,
        )
        .unwrap();
        assert_eq!(
            old.metrics.remove("upstream_cost_usd"),
            Some(decimal("0.0125"))
        );
        assert_eq!(
            old.metrics.remove("upstream_inference_cost_usd"),
            Some(decimal("0.011"))
        );
        assert_eq!(old.metrics.remove("video_input_tokens"), Some(6.into()));
        for dimension in ["upstream_priced", "is_byok", "serving_model"] {
            assert!(old.dimensions.remove(dimension).is_some(), "{dimension}");
        }
        assert_eq!(old.attempts.len(), 1);
        assert_eq!(old.attempts[0].model, "anthropic/claude-opus-4-8");
        old.attempts.clear();
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, &reply),
            Some(old)
        );
    }

    /// Extra for phase 4: DeepSeek's cache hits live at
    /// `usage.prompt_cache_hit_tokens`, so the standard reading leaves them
    /// inside the ordinary input.
    #[cfg(feature = "deepseek")]
    #[test]
    fn deepseek_agrees_except_its_own_cache_hit_field() {
        use gproxy_channel::channels::deepseek::DeepSeek;
        let reply = body(
            &json!({"usage": {"prompt_tokens": 1000, "completion_tokens": 20,
            "prompt_cache_hit_tokens": 960, "prompt_cache_miss_tokens": 40}}),
        );
        let mut old = old_whole(
            &DeepSeek,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            &reply,
        )
        .unwrap();
        assert_eq!(old.tokens.cached_input_tokens, Some(960));
        old.tokens.input_tokens = Some(1000);
        old.tokens.cached_input_tokens = None;
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, &reply),
            Some(old)
        );
        fields_it_dropped(&DeepSeek);
        streams(&DeepSeek);
    }

    /// Extra for phase 4: Moonshot's cache hits live at the top-level
    /// `usage.cached_tokens`; with the standard detail present too, both
    /// readers agree.
    #[cfg(feature = "kimi")]
    #[test]
    fn kimi_agrees_except_its_own_cache_hit_field() {
        use gproxy_channel::channels::kimi::Kimi;
        let reply = body(
            &json!({"usage": {"prompt_tokens": 500, "completion_tokens": 8,
            "cached_tokens": 480}}),
        );
        let mut old = old_whole(
            &Kimi,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            &reply,
        )
        .unwrap();
        old.tokens.input_tokens = Some(500);
        old.tokens.cached_input_tokens = None;
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, &reply),
            Some(old)
        );
        equal(
            &Kimi,
            Dialect::OpenAiChat,
            &json!({"usage": {"prompt_tokens": 100,
            "completion_tokens": 3, "cached_tokens": 60,
            "prompt_tokens_details": {"cached_tokens": 60}}}),
        );
        streams(&Kimi);
    }

    /// DashScope's chat is plain Chat Completions. Its image reply is broken
    /// today (the reader sees the native envelope, not the shaped reply that
    /// parks the counters under `dashscope_usage`); against the shaped reply
    /// the standard reading counts the images, and the input tokens and
    /// size are an extra for phase 4.
    #[cfg(feature = "dashscope")]
    #[test]
    fn dashscope_chat_agrees_and_its_image_counters_are_an_extra() {
        use gproxy_channel::channels::dashscope::DashScope;
        equal(
            &DashScope,
            Dialect::OpenAiChat,
            &json!({"usage": {"prompt_tokens": 90,
            "completion_tokens": 7, "prompt_tokens_details": {"cached_tokens": 20}}}),
        );
        fields_it_dropped(&DashScope);
        streams(&DashScope);

        let shaped = body(
            &json!({"request_id": "r1", "data": [{"url": "https://cdn.example/1.png"}],
            "dashscope_usage": {"image_count": 1, "input_tokens": 9}}),
        );
        let old = old_whole(&DashScope, Operation::CreateImage, Dialect::OpenAi, &shaped).unwrap();
        assert_eq!(old.tokens.input_tokens, Some(9));
        let new = new_whole(Operation::CreateImage, Dialect::OpenAi, &shaped).unwrap();
        assert_eq!(
            new.metrics.get("image_outputs"),
            old.metrics.get("image_outputs")
        );
        assert_eq!(new.tokens.input_tokens, None, "dashscope_usage is an extra");
    }
}

// --------------------------------------------------------------- bedrock

/// Bedrock's observer translated the AWS event stream itself; after the
/// move the standard reader watches the Messages SSE the channel already
/// produces. Its Messages reading wrapped an ordinary answer in one
/// attempt, and it read no server tools or qualifiers.
#[cfg(feature = "aws_bedrock")]
mod aws_bedrock {
    use super::*;
    use gproxy_channel::channels::aws_bedrock::AwsBedrock;

    #[test]
    fn messages_agree_except_the_single_attempt_and_dropped_qualifiers() {
        let reply = br#"{"model":"claude-x","stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":9,"cache_read_input_tokens":4,"cache_creation_input_tokens":5,"service_tier":"standard"}}"#;
        let mut old = old_whole(
            &AwsBedrock,
            Operation::GenerateContent,
            Dialect::Claude,
            reply,
        )
        .unwrap();
        drop_single_attempt(&mut old);
        old.dimensions
            .insert("service_tier".into(), "standard".into());
        old.actual_service_tier = Some("standard".into());
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::Claude, reply),
            Some(old)
        );

        let refused = sse(&[
            json!({"type": "message_start", "message": {"model": "claude-fable-5",
                "usage": {"input_tokens": 20, "output_tokens": 0}}}),
            json!({"type": "message_delta", "delta": {"stop_reason": "refusal"},
                "usage": {"output_tokens": 0}}),
            json!({"type": "message_stop"}),
        ]);
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::Claude,
                SSE,
                &refused
            ),
            old_stream(
                &AwsBedrock,
                Operation::StreamGenerateContent,
                Dialect::Claude,
                SSE,
                &refused
            )
        );
    }

    #[test]
    fn chat_agrees_except_cache_writes() {
        let chat = br#"{"usage":{"prompt_tokens":11,"completion_tokens":4,"prompt_tokens_details":{"cached_tokens":5}}}"#;
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, chat),
            old_whole(
                &AwsBedrock,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                chat
            )
        );
    }
}

// ------------------------------------------------------------- workbuddy

/// WorkBuddy's chat is `openai_wire`'s reading; its image replies are
/// broken today and fixed by phase 4.
#[cfg(feature = "workbuddy")]
mod workbuddy {
    use super::*;
    use gproxy_channel::channels::workbuddy::WorkBuddy;

    #[test]
    fn chat_bodies_and_streams_are_equal() {
        let reply = body(
            &json!({"usage": {"prompt_tokens": 500, "completion_tokens": 8,
            "prompt_tokens_details": {"cached_tokens": 480}}}),
        );
        assert_eq!(
            new_whole(Operation::GenerateContent, Dialect::OpenAiChat, &reply),
            old_whole(
                &WorkBuddy,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &reply
            )
        );
        let wire = b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: {\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n";
        assert_eq!(
            new_stream(
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                SSE,
                wire
            ),
            old_stream(
                &WorkBuddy,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                SSE,
                wire
            )
        );
    }

    #[test]
    fn an_unwrapped_image_reply_counts_its_images_alike() {
        let reply = br#"{"data":[{"b64_json":"A"},{"b64_json":"B"}]}"#;
        assert_eq!(
            new_whole(Operation::CreateImage, Dialect::OpenAi, reply),
            old_whole(&WorkBuddy, Operation::CreateImage, Dialect::OpenAi, reply)
        );
    }
}
