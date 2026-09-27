//! What each channel's replies settle with, against what the channel's own
//! usage reader used to read from them, fixture by fixture.
//!
//! Usage used to be read by each channel, from the upstream's raw bytes,
//! through a `UsageExtractor` or a `UsageStream` of its own. It is now read
//! once, by operation and dialect (`gproxy_protocol::usage`), from the
//! standard response the channel shapes, with the channel's `UsageExtras`
//! adding what only its vendor reports. The old readers are gone; this file
//! keeps what they read as fixtures. Each test settles one reply the way the
//! host does and compares the result with the old reading, after applying to
//! the old reading the differences that are meant — so every change in what
//! gets billed is written down here rather than discovered on an invoice.
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
//! - **Vendor extras keep their keys.** OpenRouter's cost, byok flag and
//!   serving model, xAI and Grok Build's cost ticks, stated dollars, video
//!   seconds and non-standard web-search field, DeepSeek's
//!   `prompt_cache_hit_tokens`, Kimi's top-level `cached_tokens`, DashScope's
//!   image counters, Devin's cache creation and served model, and AI Studio's
//!   service-tier header are read by each channel's `UsageExtras`, under the
//!   metric and dimension names pricing already uses. xAI's image input
//!   tokens are standard Responses detail and come from the standard reading.
//!
//! Channels whose old reading was broken rather than different — the reader
//! saw the upstream's bytes before the channel shaped them — settle
//! correctly now, and are tested against the shaped response in their own
//! test files: `kiro` (AWS event stream), `devin` (Connect framing and
//! protobuf), `claudeweb` (claude.ai's own events; its counts are the
//! channel's estimates and settle marked as such), and the image replies of
//! `workbuddy` and `dashscope` (the vendor envelope rather than the
//! unwrapped OpenAI reply). `aws_bedrock`, `geminicli`, `antigravity` and
//! `cline` read correctly before by undoing their framing inside the reader;
//! here their shaped response is compared with that reading.

mod support;

use gproxy_channel::{BaseChannel, channel::NormalizedUsage};
use gproxy_protocol::{Dialect, Operation};
use http::HeaderMap;
use serde_json::{Map, Value, json};

// ------------------------------------------------------------------ harness

/// A complete reply as the host settles it.
fn settled<C: BaseChannel>(
    channel: &C,
    operation: Operation,
    dialect: Dialect,
    body: &[u8],
) -> Option<NormalizedUsage> {
    support::settled(channel, operation, dialect, &HeaderMap::new(), body)
}

/// An SSE stream as the host settles it.
fn settled_stream<C: BaseChannel>(
    channel: &C,
    operation: Operation,
    dialect: Dialect,
    wire: &[u8],
) -> Option<NormalizedUsage> {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/event-stream"),
    );
    support::settled_stream(channel, operation, dialect, &headers, wire)
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

/// A reading as a fixture: every reported field, and nothing that was not
/// reported. Amounts are strings, so a decimal compares exactly.
fn summary(usage: Option<&NormalizedUsage>) -> Value {
    let Some(usage) = usage else {
        return Value::Null;
    };
    let mut out = Map::new();
    let t = &usage.tokens;
    let mut tokens = Map::new();
    for (name, value) in [
        ("input", t.input_tokens),
        ("output", t.output_tokens),
        ("cached", t.cached_input_tokens),
        ("cache_5m", t.cache_creation_5m_tokens),
        ("cache_30m", t.cache_creation_30m_tokens),
        ("cache_1h", t.cache_creation_1h_tokens),
        ("reasoning", t.reasoning_tokens),
    ] {
        if let Some(value) = value {
            tokens.insert(name.into(), value.into());
        }
    }
    out.insert("tokens".into(), Value::Object(tokens));
    if !usage.metrics.is_empty() {
        out.insert(
            "metrics".into(),
            usage
                .metrics
                .iter()
                .map(|(key, value)| (key.clone(), Value::from(value.normalize().to_string())))
                .collect(),
        );
    }
    if !usage.dimensions.is_empty() {
        out.insert("dimensions".into(), json!(usage.dimensions));
    }
    if let Some(tier) = &usage.actual_service_tier {
        out.insert("tier".into(), tier.as_str().into());
    }
    out.insert(
        "completeness".into(),
        format!("{:?}", usage.completeness).into(),
    );
    if !usage.attempts.is_empty() {
        out.insert(
            "attempts".into(),
            usage
                .attempts
                .iter()
                .map(|attempt| {
                    json!({"model": attempt.model, "billable": attempt.billable,
                        "output": attempt.usage.tokens.output_tokens})
                })
                .collect(),
        );
    }
    Value::Object(out)
}

/// Compare what a reply settles with against what the old reader read,
/// after `change` applies the intended differences to the old reading.
#[track_caller]
fn check(new: Option<NormalizedUsage>, mut old: Value, change: impl FnOnce(&mut Value)) {
    change(&mut old);
    assert_eq!(summary(new.as_ref()), old);
}

/// No intended difference.
fn same(_: &mut Value) {}

/// The standard reading marks modality metrics as subsets of the totals.
fn flag_modalities(old: &mut Value) {
    old["dimensions"]["token_modalities_in_totals"] = "true".into();
}

/// `vendor_usage` and `aws_bedrock` wrapped an ordinary Claude answer in one
/// attempt naming its model; the standard reading leaves the exchange's
/// model to price it.
fn drop_single_attempt(old: &mut Value) {
    let attempts = old
        .as_object_mut()
        .unwrap()
        .remove("attempts")
        .expect("the old reading had an attempt");
    assert_eq!(attempts.as_array().unwrap().len(), 1);
    assert_eq!(attempts[0]["billable"], true);
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
        check(
            settled(
                &OpenAi,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &chat,
            ),
            json!({"tokens": {"input": 57, "output": 20, "cached": 40, "cache_30m": 3,
                "reasoning": 7}, "metrics": {"audio_input_tokens": "5"},
                "dimensions": {"service_tier": "flex"}, "tier": "flex",
                "completeness": "Complete"}),
            flag_modalities,
        );

        let responses = body(&json!({"usage": {"input_tokens": 11, "output_tokens": 4,
            "input_tokens_details": {"cached_tokens": 2},
            "output_tokens_details": {"reasoning_tokens": 1},
            "server_tool_use": {"web_search_requests": 3}}}));
        check(
            settled(
                &OpenAi,
                Operation::GenerateContent,
                Dialect::OpenAi,
                &responses,
            ),
            json!({"tokens": {"input": 9, "output": 4, "cached": 2, "reasoning": 1},
                "metrics": {"web_searches": "3"}, "completeness": "Complete"}),
            same,
        );
        check(
            settled(
                &OpenAi,
                Operation::GenerateContent,
                Dialect::OpenAi,
                br#"{"id":"resp_1"}"#,
            ),
            Value::Null,
            same,
        );
    }

    /// `openai_wire` took the image tokens out of `output_tokens`; the
    /// standard reading keeps the total whole and flags the image metric as
    /// a subset of it, which prices the same whenever image output has a
    /// price of its own.
    #[test]
    fn an_image_reply_keeps_its_output_total_whole() {
        let reply = br#"{"data":[{"b64_json":"x"}],"usage":{"input_tokens":2,"output_tokens":8}}"#;
        check(
            settled(&OpenAi, Operation::CreateImage, Dialect::OpenAi, reply),
            json!({"tokens": {"input": 2, "output": 0},
                "metrics": {"image_output_tokens": "8", "image_outputs": "1"},
                "completeness": "Complete"}),
            |old| {
                old["tokens"]["output"] = 8.into();
                flag_modalities(old);
            },
        );
    }

    #[test]
    fn a_transcription_is_equal() {
        check(
            settled(
                &OpenAi,
                Operation::CreateTranscription,
                Dialect::OpenAi,
                br#"{"text":"hi","usage":{"type":"tokens","input_tokens":14,"output_tokens":45}}"#,
            ),
            json!({"tokens": {"input": 14, "output": 45}, "completeness": "Complete"}),
            same,
        );
        check(
            settled(
                &OpenAi,
                Operation::CreateTranscription,
                Dialect::OpenAi,
                br#"{"text":"hi","usage":{"type":"duration","seconds":3.5}}"#,
            ),
            json!({"tokens": {}, "metrics": {"audio_seconds": "3.5"},
                "completeness": "Complete"}),
            same,
        );
    }

    #[test]
    fn chat_and_responses_streams_are_equal() {
        let chat = [
            &b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n"[..],
            b"data: {\"usage\":{\"prompt_tokens\":9,\"completion_",
            b"tokens\":4}}\n\ndata: [DONE]\n\n",
        ]
        .concat();
        check(
            settled_stream(
                &OpenAi,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                &chat,
            ),
            json!({"tokens": {"input": 9, "output": 4}, "completeness": "Complete"}),
            same,
        );
        let responses = [
            &b"event: response.in_progress\ndata: {\"type\":\"response.in_progress\"}\n\n"[..],
            b"event: response.com",
            b"pleted\r\ndata: {\"type\":\"response.completed\",\"response\":{\"service_tier\":\"priority\",\"usage\":{",
            b"\"input_tokens\":9,\"output_tokens\":4}}}\r\n\r\n",
        ]
        .concat();
        check(
            settled_stream(
                &OpenAi,
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                &responses,
            ),
            json!({"tokens": {"input": 9, "output": 4},
                "dimensions": {"service_tier": "priority"}, "tier": "priority",
                "completeness": "Complete"}),
            same,
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
        check(
            settled(
                &Claudeapi,
                Operation::GenerateContent,
                Dialect::Claude,
                &messages,
            ),
            json!({"tokens": {"input": 100, "output": 50, "cached": 40, "cache_5m": 7,
                "cache_1h": 3, "reasoning": 12}, "metrics": {"web_searches": "2"},
                "dimensions": {"service_tier": "standard"}, "tier": "standard",
                "completeness": "Complete"}),
            same,
        );
        let chat = br#"{"usage":{"prompt_tokens":11,"completion_tokens":4,"prompt_tokens_details":{"cached_tokens":5}}}"#;
        check(
            settled(
                &Claudeapi,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                chat,
            ),
            json!({"tokens": {"input": 6, "output": 4, "cached": 5},
                "completeness": "Complete"}),
            same,
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
        check(
            settled_stream(
                &Claudeapi,
                Operation::StreamGenerateContent,
                Dialect::Claude,
                &wire,
            ),
            json!({"tokens": {"input": 30, "output": 19, "cached": 4},
                "dimensions": {"service_tier": "priority"}, "tier": "priority",
                "completeness": "Complete"}),
            same,
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
        check(
            settled_stream(
                &Claudecode,
                Operation::StreamGenerateContent,
                Dialect::Claude,
                wire,
            ),
            json!({"tokens": {"input": 25, "output": 12, "cached": 10, "cache_5m": 0,
                "cache_1h": 20, "reasoning": 4}, "completeness": "Complete",
                "attempts": [
                    {"model": "claude-fable-5", "billable": false, "output": 0},
                    {"model": "claude-opus-4-8", "billable": true, "output": 12}]}),
            same,
        );

        let reply = body(
            &json!({"model": "claude-sonnet-4-6", "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 4, "cache_read_input_tokens": 30,
                "cache_creation": {"ephemeral_5m_input_tokens": 2, "ephemeral_1h_input_tokens": 3},
                "output_tokens_details": {"thinking_tokens": 1},
                "server_tool_use": {"web_search_requests": 2, "web_fetch_requests": 1},
                "service_tier": "standard", "speed": "fast"}}),
        );
        check(
            settled(
                &Claudecode,
                Operation::GenerateContent,
                Dialect::Claude,
                &reply,
            ),
            json!({"tokens": {"input": 10, "output": 4, "cached": 30, "cache_5m": 2,
                "cache_1h": 3, "reasoning": 1},
                "metrics": {"web_fetches": "1", "web_searches": "2"},
                "dimensions": {"service_tier": "standard", "speed": "fast"},
                "tier": "standard", "completeness": "Complete"}),
            same,
        );
    }

    /// The claudecode reader dropped a refusal on the floor; a refused
    /// answer is now one attempt, billable only if it produced output — the
    /// rule `vendor_usage` and `aws_bedrock` already applied.
    #[test]
    fn a_refusal_now_becomes_an_unbillable_attempt() {
        let reply = br#"{"model":"claude-x","stop_reason":"refusal","usage":{"input_tokens":9,"output_tokens":0}}"#;
        check(
            settled(
                &Claudecode,
                Operation::GenerateContent,
                Dialect::Claude,
                reply,
            ),
            json!({"tokens": {"input": 9, "output": 0}, "completeness": "Complete"}),
            |old| {
                old["attempts"] = json!([{"model": "claude-x", "billable": false, "output": 0}]);
            },
        );
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
        check(
            settled(
                &Aistudio,
                Operation::GenerateContent,
                Dialect::Gemini,
                &reply,
            ),
            json!({"tokens": {"input": 70, "output": 30, "cached": 30, "reasoning": 20},
                "metrics": {"image_output_tokens": "40"},
                "dimensions": {"service_tier": "flex"}, "tier": "flex",
                "completeness": "Complete"}),
            |old| {
                old["tokens"]["output"] = 70.into();
                flag_modalities(old);
                old["metrics"]["tool_use_prompt_tokens"] = "4".into();
            },
        );
    }

    #[test]
    fn an_embedding_is_equal() {
        let reply = br#"{"embedding":{"values":[0.1]},"usageMetadata":{"promptTokenCount":6}}"#;
        check(
            settled(
                &Aistudio,
                Operation::CreateEmbedding,
                Dialect::Gemini,
                reply,
            ),
            json!({"tokens": {"input": 6}, "completeness": "Complete"}),
            same,
        );
    }

    /// AI Studio ignored a record without a candidate count, so a stream cut
    /// after it read nothing; the standard reading reports the prompt with
    /// the output left unknown for the estimator. A stream that ends on its
    /// own reads the same either way, framed as SSE or as a JSON array.
    #[test]
    fn streams_are_equal_once_settled() {
        let partial = r#"{"usageMetadata":{"promptTokenCount":100}}"#;
        let last = r#"{"usageMetadata":{"promptTokenCount":100,"candidatesTokenCount":50,"thoughtsTokenCount":10}}"#;
        let old = json!({"tokens": {"input": 100, "output": 60, "cached": 0, "reasoning": 10},
            "completeness": "Complete"});
        let sse = format!("data: {partial}\n\ndata: {last}\n\n");
        check(
            settled_stream(
                &Aistudio,
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                sse.as_bytes(),
            ),
            old.clone(),
            same,
        );
        // Without `alt=sse` the records arrive as one JSON array.
        let array = format!("[{partial},{last}]");
        check(
            support::settled_stream(
                &Aistudio,
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                &HeaderMap::new(),
                array.as_bytes(),
            ),
            old,
            same,
        );
    }

    /// AI Studio names the tier it served in the `x-gemini-service-tier`
    /// header, which no body reader sees; its extras still read it.
    #[test]
    fn the_service_tier_header_is_an_extra() {
        let reply = br#"{"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1}}"#;
        let mut headers = HeaderMap::new();
        headers.insert("x-gemini-service-tier", "flex".parse().unwrap());
        check(
            support::settled(
                &Aistudio,
                Operation::GenerateContent,
                Dialect::Gemini,
                &headers,
                reply,
            ),
            json!({"tokens": {"input": 1, "output": 1, "cached": 0},
                "dimensions": {"service_tier": "flex"}, "tier": "flex",
                "completeness": "Complete"}),
            same,
        );
    }
}

// ------------------------------------------------- code assist (gemini)

/// Code Assist wraps each Gemini reply under `response`, and the channel
/// shaping takes the wrapper off, so the reply settles unwrapped (the
/// channels' own tests run the shaping). The shared Code Assist reader
/// reported a zero reasoning count where Gemini reported none, and did not
/// flag its media metrics.
#[cfg(any(feature = "geminicli", feature = "antigravity"))]
fn code_assist_parity<C: BaseChannel>(channel: &C) {
    let inner = json!({"usageMetadata": {"promptTokenCount": 100, "cachedContentTokenCount": 40,
        "candidatesTokenCount": 10, "thoughtsTokenCount": 5,
        "candidatesTokensDetails": [{"modality": "IMAGE", "tokenCount": 4}]}});
    check(
        settled(
            channel,
            Operation::GenerateContent,
            Dialect::Gemini,
            &body(&inner),
        ),
        json!({"tokens": {"input": 60, "output": 15, "cached": 40, "reasoning": 5},
            "metrics": {"image_output_tokens": "4"}, "completeness": "Complete"}),
        flag_modalities,
    );

    let records = [
        json!({"usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 1}}),
        json!({"usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 7}}),
    ];
    check(
        settled_stream(
            channel,
            Operation::StreamGenerateContent,
            Dialect::Gemini,
            &sse(&records),
        ),
        json!({"tokens": {"input": 9, "output": 7, "cached": 0, "reasoning": 0},
            "completeness": "Complete"}),
        |old| {
            old["tokens"].as_object_mut().unwrap().remove("reasoning");
        },
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
            let old = json!({"tokens": {"input": 57, "output": 20, "cached": 40,
                "cache_30m": 3}, "completeness": "Complete"});
            let reply = body(&json!({"usage": usage}));
            check(
                settled(&Codex, Operation::GenerateContent, Dialect::OpenAi, &reply),
                old.clone(),
                same,
            );
            let wire = sse(&[
                json!({"type": "response.output_text.delta", "delta": "hi"}),
                json!({"type": "response.completed", "response": {"id": "r1", "usage": usage}}),
            ]);
            check(
                settled_stream(
                    &Codex,
                    Operation::StreamGenerateContent,
                    Dialect::OpenAi,
                    &wire,
                ),
                old,
                same,
            );
        }
    }

    /// Codex dropped web-search counts; the standard reading keeps them.
    #[test]
    fn web_searches_are_now_kept() {
        let reply = body(&json!({"usage": {"input_tokens": 5, "output_tokens": 1,
            "server_tool_use": {"web_search_requests": 2}}}));
        check(
            settled(&Codex, Operation::GenerateContent, Dialect::OpenAi, &reply),
            json!({"tokens": {"input": 5, "output": 1}, "completeness": "Complete"}),
            |old| old["metrics"] = json!({"web_searches": "2"}),
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
            check(
                settled_stream(&Codex, operation, Dialect::OpenAi, wire.as_bytes()),
                json!({"tokens": {"input": 15, "output": 10},
                    "metrics": {"image_input_tokens": "12", "image_output_tokens": "10",
                        "image_outputs": "1", "text_input_tokens": "3"},
                    "dimensions": {"quality": "high", "size": "1024x1024",
                        "token_modalities_in_totals": "true"},
                    "completeness": "Complete"}),
                same,
            );
        }
    }

    /// A realtime session: each `response.done` is read once by its id,
    /// and the session is their sum.
    #[test]
    fn a_realtime_session_is_equal() {
        use gproxy_protocol::usage::{UsageReader, UsageStreamEnd, UsageTransport};
        let done = json!({"type": "response.done", "response": {"id": "r1", "usage": {
            "input_tokens": 100, "output_tokens": 30,
            "input_token_details": {"cached_tokens": 20, "audio_tokens": 70, "text_tokens": 30,
                "cached_tokens_details": {"audio_tokens": 15, "text_tokens": 5}},
            "output_token_details": {"audio_tokens": 25, "text_tokens": 5}}}})
        .to_string();
        let second = json!({"type": "response.done", "response": {"id": "r2",
            "usage": {"input_tokens": 10, "output_tokens": 4}}})
        .to_string();
        let mut reader = UsageReader::new(
            Operation::ConnectRealtime,
            Dialect::OpenAi,
            UsageTransport::WebSocket,
        )
        .unwrap();
        for text in [&done, &done, &second] {
            reader.push_message(text);
        }
        let usage = reader.finish(UsageStreamEnd::Complete).unwrap();
        assert_eq!(usage.responses.len(), 2);
        check(
            Some(usage),
            json!({"tokens": {"input": 90, "output": 34, "cached": 20},
                "metrics": {"audio_input_tokens": "70", "audio_output_tokens": "25",
                    "cached_audio_input_tokens": "15", "cached_text_input_tokens": "5",
                    "text_input_tokens": "30", "text_output_tokens": "5"},
                "completeness": "Complete"}),
            same,
        );
    }
}

// -------------------------------------------- vendor_usage passthroughs

/// Azure, Vertex, Vertex Express, Custom, Vercel, NVIDIA and the Cloudflare
/// AI Gateway forward the vendor's own body, which `shared::vendor_usage`
/// read — streams included, by buffering the whole stream and scanning it.
/// Their streams are now watched as they pass.
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

    const CHAT: &str = r#"{"choices":[{"message":{"content":"hi"}}],"usage":{"prompt_tokens":150,"completion_tokens":40,"prompt_tokens_details":{"cached_tokens":50},"completion_tokens_details":{"reasoning_tokens":12}}}"#;
    const RESPONSES: &str = r#"{"usage":{"input_tokens":120,"output_tokens":30,"input_tokens_details":{"cached_tokens":20},"output_tokens_details":{"reasoning_tokens":10}}}"#;
    const GEMINI: &str = r#"{"usageMetadata":{"promptTokenCount":200,"candidatesTokenCount":60,"thoughtsTokenCount":25,"cachedContentTokenCount":80,"toolUsePromptTokenCount":13}}"#;

    fn chat<C: BaseChannel>(channel: &C) {
        check(
            settled(
                channel,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                CHAT.as_bytes(),
            ),
            json!({"tokens": {"input": 100, "output": 40, "cached": 50, "reasoning": 12},
                "completeness": "Complete"}),
            same,
        );
    }

    fn responses<C: BaseChannel>(channel: &C) {
        check(
            settled(
                channel,
                Operation::GenerateContent,
                Dialect::OpenAi,
                RESPONSES.as_bytes(),
            ),
            json!({"tokens": {"input": 100, "output": 30, "cached": 20, "reasoning": 10},
                "completeness": "Complete"}),
            same,
        );
    }

    fn gemini<C: BaseChannel>(channel: &C) {
        check(
            settled(
                channel,
                Operation::GenerateContent,
                Dialect::Gemini,
                GEMINI.as_bytes(),
            ),
            json!({"tokens": {"input": 120, "output": 85, "cached": 80, "reasoning": 25},
                "metrics": {"tool_use_prompt_tokens": "13"}, "completeness": "Complete"}),
            same,
        );
    }

    /// `vendor_usage` left a cache write inside the ordinary input and
    /// dropped audio details, web searches and the serving tier.
    fn fields_it_dropped<C: BaseChannel>(channel: &C) {
        let reply = br#"{"service_tier":"priority","usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":40,"cache_write_tokens":3,"audio_tokens":5},"server_tool_use":{"web_search_requests":2}}}"#;
        check(
            settled(
                channel,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                reply,
            ),
            json!({"tokens": {"input": 60, "output": 20, "cached": 40},
                "completeness": "Complete"}),
            |old| {
                old["tokens"]["input"] = 57.into();
                old["tokens"]["cache_30m"] = 3.into();
                old["metrics"] = json!({"audio_input_tokens": "5", "web_searches": "2"});
                flag_modalities(old);
                old["dimensions"]["service_tier"] = "priority".into();
                old["tier"] = "priority".into();
            },
        );
    }

    /// A Claude reply: `vendor_usage` wrapped it in one attempt and ignored
    /// server tools and qualifiers. A refusal was an unbillable attempt
    /// already.
    fn claude<C: BaseChannel>(channel: &C) {
        let reply = br#"{"model":"claude-x","stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":9,"cache_read_input_tokens":4,"cache_creation":{"ephemeral_5m_input_tokens":5,"ephemeral_1h_input_tokens":6},"server_tool_use":{"web_search_requests":1,"web_fetch_requests":0},"speed":"fast"}}"#;
        check(
            settled(channel, Operation::GenerateContent, Dialect::Claude, reply),
            json!({"tokens": {"input": 7, "output": 9, "cached": 4, "cache_5m": 5,
                "cache_1h": 6}, "completeness": "Complete",
                "attempts": [{"model": "claude-x", "billable": true, "output": 9}]}),
            |old| {
                drop_single_attempt(old);
                old["metrics"] = json!({"web_searches": "1"});
                old["dimensions"] = json!({"speed": "fast"});
            },
        );
        let refused = br#"{"model":"claude-x","stop_reason":"refusal","usage":{"input_tokens":7,"output_tokens":0}}"#;
        check(
            settled(
                channel,
                Operation::GenerateContent,
                Dialect::Claude,
                refused,
            ),
            json!({"tokens": {"input": 7, "output": 0}, "completeness": "Complete",
                "attempts": [{"model": "claude-x", "billable": false, "output": 0}]}),
            same,
        );
    }

    /// The watched streams read what the accumulated ones did, except for
    /// the single attempt on an ordinary Claude answer and Gemini's
    /// explicit zero cache read.
    fn streams<C: BaseChannel>(channel: &C, gemini: bool) {
        let claude = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-x\",\"usage\":{\"input_tokens\":11,\"cache_read_input_tokens\":4}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
        );
        check(
            settled_stream(
                channel,
                Operation::StreamGenerateContent,
                Dialect::Claude,
                claude.as_bytes(),
            ),
            json!({"tokens": {"input": 11, "output": 5, "cached": 4},
                "completeness": "Complete",
                "attempts": [{"model": "claude-x", "billable": true, "output": 5}]}),
            drop_single_attempt,
        );

        let chat = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}],\"usage\":null}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":31,\"completion_tokens\":17,\"prompt_tokens_details\":{\"cached_tokens\":9}}}\n\n",
            "data: [DONE]\n\n",
        );
        check(
            settled_stream(
                channel,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                chat.as_bytes(),
            ),
            json!({"tokens": {"input": 22, "output": 17, "cached": 9},
                "completeness": "Complete"}),
            same,
        );

        if gemini {
            let wire = concat!(
                "data: {\"usageMetadata\":{\"promptTokenCount\":40,\"candidatesTokenCount\":2}}\n\n",
                "data: {\"usageMetadata\":{\"promptTokenCount\":40,\"candidatesTokenCount\":18}}\n\n",
            );
            check(
                settled_stream(
                    channel,
                    Operation::StreamGenerateContent,
                    Dialect::Gemini,
                    wire.as_bytes(),
                ),
                json!({"tokens": {"input": 40, "output": 18}, "completeness": "Complete"}),
                // Gemini leaves out a zero cache read; a settled reply states it.
                |old| old["tokens"]["cached"] = 0.into(),
            );
        }
    }

    #[cfg(feature = "azure")]
    #[test]
    fn azure_agrees_except_cache_writes_dropped_fields_and_single_attempts() {
        use gproxy_channel::channels::azure::Azure;
        chat(&Azure);
        responses(&Azure);
        fields_it_dropped(&Azure);
        claude(&Azure);
        streams(&Azure, false);
    }

    #[cfg(feature = "vertex")]
    #[test]
    fn vertex_agrees_except_single_attempts_and_the_explicit_zero_cache() {
        use gproxy_channel::channels::vertex::Vertex;
        gemini(&Vertex);
        claude(&Vertex);
        streams(&Vertex, true);
    }

    #[cfg(feature = "vertexexpress")]
    #[test]
    fn vertexexpress_agrees() {
        use gproxy_channel::channels::vertexexpress::VertexExpress;
        gemini(&VertexExpress);
    }

    #[cfg(feature = "custom")]
    #[test]
    fn custom_agrees_except_cache_writes_dropped_fields_and_single_attempts() {
        use gproxy_channel::channels::custom::Custom;
        chat(&Custom);
        responses(&Custom);
        gemini(&Custom);
        fields_it_dropped(&Custom);
        claude(&Custom);
        streams(&Custom, true);
    }

    #[cfg(feature = "vercel")]
    #[test]
    fn vercel_agrees_except_cache_writes_dropped_fields_and_single_attempts() {
        use gproxy_channel::channels::vercel::Vercel;
        chat(&Vercel);
        responses(&Vercel);
        gemini(&Vercel);
        fields_it_dropped(&Vercel);
        claude(&Vercel);
        streams(&Vercel, true);
    }

    #[cfg(feature = "nvidia")]
    #[test]
    fn nvidia_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::nvidia::Nvidia;
        chat(&Nvidia);
        fields_it_dropped(&Nvidia);
    }

    #[cfg(feature = "cloudflare_ai_gateway")]
    #[test]
    fn cloudflare_ai_gateway_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::cloudflare_ai_gateway::CloudflareAiGateway;
        chat(&CloudflareAiGateway);
        responses(&CloudflareAiGateway);
        gemini(&CloudflareAiGateway);
        fields_it_dropped(&CloudflareAiGateway);
    }
}

// ---------------------------------------------- shared::compatible users

/// Copilot CLI, OpenCode, Cline, xAI, Grok Build, OpenRouter, DeepSeek, Kimi
/// and DashScope read through `shared::compatible::usage` and a per-channel
/// hook for their own fields; the hooks are their `UsageExtras` now.
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

    /// `compatible` left a cache write inside the ordinary input and
    /// dropped audio details, web searches and the serving tier, as
    /// `vendor_usage` did. Only the standard part is compared: a vendor's
    /// extras may add keys of their own.
    fn fields_it_dropped<C: BaseChannel>(channel: &C) {
        let reply = br#"{"service_tier":"priority","usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":40,"cache_write_tokens":3,"audio_tokens":5},"server_tool_use":{"web_search_requests":2}}}"#;
        let new = settled(
            channel,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            reply,
        );
        let mut new = summary(new.as_ref());
        let standard = [
            "audio_input_tokens",
            "web_searches",
            "service_tier",
            "token_modalities_in_totals",
        ];
        for key in ["metrics", "dimensions"] {
            if let Some(fields) = new.get_mut(key).and_then(Value::as_object_mut) {
                fields.retain(|name, _| standard.contains(&name.as_str()));
            }
        }
        let mut old = json!({"tokens": {"input": 60, "output": 20, "cached": 40},
            "completeness": "Complete"});
        old["tokens"]["input"] = 57.into();
        old["tokens"]["cache_30m"] = 3.into();
        old["metrics"] = json!({"audio_input_tokens": "5", "web_searches": "2"});
        flag_modalities(&mut old);
        old["dimensions"]["service_tier"] = "priority".into();
        old["tier"] = "priority".into();
        assert_eq!(new, old);
    }

    /// Chat and Responses streams settle on the same reading.
    fn streams<C: BaseChannel>(channel: &C) {
        let chat = sse(&[
            json!({"choices": [{"delta": {"content": "hi"}}], "usage": null}),
            json!({"choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 2,
                "prompt_tokens_details": {"cached_tokens": 4}}}),
        ]);
        check(
            settled_stream(
                channel,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                &chat,
            ),
            json!({"tokens": {"input": 6, "output": 2, "cached": 4},
                "completeness": "Complete"}),
            same,
        );
        let responses = sse(&[
            json!({"type": "response.output_text.delta", "delta": "hi"}),
            json!({"type": "response.completed", "response": {"usage":
                {"input_tokens": 9, "output_tokens": 3}}}),
        ]);
        check(
            settled_stream(
                channel,
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                &responses,
            ),
            json!({"tokens": {"input": 9, "output": 3}, "completeness": "Complete"}),
            same,
        );
    }

    /// A Claude reply: `compatible` ignored server tools and qualifiers.
    fn claude<C: BaseChannel>(channel: &C) {
        let reply = br#"{"usage":{"input_tokens":100,"output_tokens":4,"cache_read_input_tokens":40,"server_tool_use":{"web_search_requests":1},"service_tier":"standard"}}"#;
        check(
            settled(channel, Operation::GenerateContent, Dialect::Claude, reply),
            json!({"tokens": {"input": 100, "output": 4, "cached": 40},
                "completeness": "Complete"}),
            |old| {
                old["metrics"] = json!({"web_searches": "1"});
                old["dimensions"] = json!({"service_tier": "standard"});
                old["tier"] = "standard".into();
            },
        );
    }

    #[cfg(feature = "copilotcli")]
    #[test]
    fn copilotcli_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::copilotcli::CopilotCli;
        check(
            settled(
                &CopilotCli,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(
                    &json!({"usage": {"prompt_tokens": 90, "completion_tokens": 7,
                    "prompt_tokens_details": {"cached_tokens": 20}}}),
                ),
            ),
            json!({"tokens": {"input": 70, "output": 7, "cached": 20},
                "completeness": "Complete"}),
            same,
        );
        fields_it_dropped(&CopilotCli);
        streams(&CopilotCli);
    }

    #[cfg(feature = "opencode")]
    #[test]
    fn opencode_agrees_except_cache_writes_and_dropped_fields() {
        use gproxy_channel::channels::opencode::OpenCode;
        check(
            settled(
                &OpenCode::ZEN,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(
                    &json!({"usage": {"prompt_tokens": 100, "completion_tokens": 4,
                    "prompt_tokens_details": {"cached_tokens": 40}}}),
                ),
            ),
            json!({"tokens": {"input": 60, "output": 4, "cached": 40},
                "completeness": "Complete"}),
            same,
        );
        check(
            settled(
                &OpenCode::ZEN,
                Operation::GenerateContent,
                Dialect::Claude,
                &body(&json!({"usage": {"input_tokens": 100, "output_tokens": 4,
                    "cache_read_input_tokens": 40}})),
            ),
            json!({"tokens": {"input": 100, "output": 4, "cached": 40},
                "completeness": "Complete"}),
            same,
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
        check(
            settled(
                &Cline,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(&inner),
            ),
            json!({"tokens": {"input": 70, "output": 4, "cached": 30},
                "completeness": "Complete"}),
            same,
        );
        fields_it_dropped(&Cline);
        streams(&Cline);
    }

    /// Extras: cost ticks, a stated price, video seconds and the
    /// non-standard `server_side_tool_usage_details` web-search count. The
    /// image input tokens are standard Responses detail, now flagged as a
    /// modality inside the input.
    fn xai_extras<C: BaseChannel>(channel: &C) {
        let reply = body(&json!({"usage": {"input_tokens": 100, "output_tokens": 5,
            "input_tokens_details": {"cached_tokens": 40, "image_tokens": 12},
            "cost_in_usd_ticks": 1234,
            "server_side_tool_usage_details": {"web_search_requests": 2}}}));
        check(
            settled(channel, Operation::GenerateContent, Dialect::OpenAi, &reply),
            json!({"tokens": {"input": 60, "output": 5, "cached": 40},
                "metrics": {"cost_in_usd_ticks": "1234", "image_input_tokens": "12",
                    "web_searches": "2"},
                "completeness": "Complete"}),
            flag_modalities,
        );

        let job = body(&json!({"usage": {"input_tokens": 4, "output_tokens": 0},
            "cost_usd": 0.4, "duration": 6}));
        check(
            settled(channel, Operation::GenerateContent, Dialect::OpenAi, &job),
            json!({"tokens": {"input": 4, "output": 0},
                "metrics": {"upstream_cost_usd": "0.4", "video_seconds": "6"},
                "dimensions": {"upstream_priced": "true"},
                "completeness": "Complete"}),
            same,
        );
    }

    #[cfg(feature = "xai")]
    #[test]
    fn xai_agrees_with_its_cost_ticks_stated_dollars_and_video_seconds() {
        use gproxy_channel::channels::xai::Xai;
        xai_extras(&Xai);
        streams(&Xai);
    }

    #[cfg(feature = "grokbuild")]
    #[test]
    fn grokbuild_agrees_with_its_cost_ticks_stated_dollars_and_video_seconds() {
        use gproxy_channel::channels::grokbuild::GrokBuild;
        xai_extras(&GrokBuild);
        streams(&GrokBuild);
    }

    /// Extras: the charged cost and its breakdown, the byok flag, the
    /// serving model (as a dimension and as the one attempt), and the video
    /// input tokens OpenRouter reports in the Chat details — in a buffered
    /// reply and in a stream's last usage chunk alike.
    #[cfg(feature = "openrouter")]
    #[test]
    fn openrouter_agrees_with_its_cost_byok_and_serving_model() {
        use gproxy_channel::channels::openrouter::OpenRouter;
        let usage = json!({"prompt_tokens": 100, "completion_tokens": 20,
            "prompt_tokens_details": {"cached_tokens": 30, "video_tokens": 6},
            "cost": 0.0125, "cost_details": {"upstream_inference_cost": 0.011},
            "is_byok": true});
        let old = json!({"tokens": {"input": 70, "output": 20, "cached": 30},
            "metrics": {"upstream_cost_usd": "0.0125", "upstream_inference_cost_usd": "0.011",
                "video_input_tokens": "6"},
            "dimensions": {"is_byok": "true", "serving_model": "anthropic/claude-opus-4-8",
                "upstream_priced": "true"},
            "completeness": "Complete",
            "attempts": [{"model": "anthropic/claude-opus-4-8", "billable": null, "output": 20}]});
        let reply = body(&json!({"model": "anthropic/claude-opus-4-8", "usage": usage}));
        check(
            settled(
                &OpenRouter,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &reply,
            ),
            old.clone(),
            same,
        );
        let wire = sse(&[
            json!({"model": "anthropic/claude-opus-4-8", "choices": [{"delta": {"content": "hi"}}],
                "usage": null}),
            json!({"model": "anthropic/claude-opus-4-8", "choices": [], "usage": usage}),
        ]);
        check(
            settled_stream(
                &OpenRouter,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                &wire,
            ),
            old,
            same,
        );
    }

    /// Extra: DeepSeek's cache hits live at `usage.prompt_cache_hit_tokens`,
    /// which its extras take out of the ordinary input.
    #[cfg(feature = "deepseek")]
    #[test]
    fn deepseek_agrees_with_its_own_cache_hit_field() {
        use gproxy_channel::channels::deepseek::DeepSeek;
        let usage = json!({"prompt_tokens": 1000, "completion_tokens": 20,
            "prompt_cache_hit_tokens": 960, "prompt_cache_miss_tokens": 40});
        let old = json!({"tokens": {"input": 40, "output": 20, "cached": 960},
            "completeness": "Complete"});
        check(
            settled(
                &DeepSeek,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(&json!({"usage": usage})),
            ),
            old.clone(),
            same,
        );
        check(
            settled_stream(
                &DeepSeek,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                &sse(&[json!({"choices": [], "usage": usage})]),
            ),
            old,
            same,
        );
        fields_it_dropped(&DeepSeek);
        streams(&DeepSeek);
    }

    /// Extra: Moonshot's cache hits live at the top-level
    /// `usage.cached_tokens`; with the standard detail present too, it is
    /// not taken out twice.
    #[cfg(feature = "kimi")]
    #[test]
    fn kimi_agrees_with_its_own_cache_hit_field() {
        use gproxy_channel::channels::kimi::Kimi;
        check(
            settled(
                &Kimi,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(
                    &json!({"usage": {"prompt_tokens": 500, "completion_tokens": 8,
                    "cached_tokens": 480}}),
                ),
            ),
            json!({"tokens": {"input": 20, "output": 8, "cached": 480},
                "completeness": "Complete"}),
            same,
        );
        check(
            settled(
                &Kimi,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(
                    &json!({"usage": {"prompt_tokens": 100, "completion_tokens": 3,
                    "cached_tokens": 60, "prompt_tokens_details": {"cached_tokens": 60}}}),
                ),
            ),
            json!({"tokens": {"input": 40, "output": 3, "cached": 60},
                "completeness": "Complete"}),
            same,
        );
        streams(&Kimi);
    }

    /// DashScope's chat is plain Chat Completions. Its image reply was
    /// broken (the reader saw the native envelope, not the shaped reply that
    /// parks the counters under `dashscope_usage`); the shaped reply settles
    /// with its extras' counts.
    #[cfg(feature = "dashscope")]
    #[test]
    fn dashscope_chat_agrees_and_its_image_counters_are_an_extra() {
        use gproxy_channel::channels::dashscope::DashScope;
        check(
            settled(
                &DashScope,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(
                    &json!({"usage": {"prompt_tokens": 90, "completion_tokens": 7,
                    "prompt_tokens_details": {"cached_tokens": 20}}}),
                ),
            ),
            json!({"tokens": {"input": 70, "output": 7, "cached": 20},
                "completeness": "Complete"}),
            same,
        );
        fields_it_dropped(&DashScope);
        streams(&DashScope);

        let shaped = body(
            &json!({"request_id": "r1", "data": [{"url": "https://cdn.example/1.png"}],
            "dashscope_usage": {"image_count": 1, "input_tokens": 9}}),
        );
        check(
            settled(&DashScope, Operation::CreateImage, Dialect::OpenAi, &shaped),
            json!({"tokens": {"input": 9}, "metrics": {"image_outputs": "1"},
                "completeness": "Complete"}),
            same,
        );
    }
}

// --------------------------------------------------------------- bedrock

/// Bedrock's observer translated the AWS event stream itself; now the
/// Messages SSE the channel produces is what settles (the channel's own
/// tests run the translation). Its Messages reading wrapped an ordinary
/// answer in one attempt, and it read no qualifiers.
#[cfg(feature = "aws_bedrock")]
mod aws_bedrock {
    use super::*;
    use gproxy_channel::channels::aws_bedrock::AwsBedrock;

    #[test]
    fn messages_agree_except_the_single_attempt_and_dropped_qualifiers() {
        let reply = br#"{"model":"claude-x","stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":9,"cache_read_input_tokens":4,"cache_creation_input_tokens":5,"service_tier":"standard"}}"#;
        check(
            settled(
                &AwsBedrock,
                Operation::GenerateContent,
                Dialect::Claude,
                reply,
            ),
            json!({"tokens": {"input": 7, "output": 9, "cached": 4, "cache_5m": 5},
                "completeness": "Complete",
                "attempts": [{"model": "claude-x", "billable": true, "output": 9}]}),
            |old| {
                drop_single_attempt(old);
                old["dimensions"] = json!({"service_tier": "standard"});
                old["tier"] = "standard".into();
            },
        );

        let refused = sse(&[
            json!({"type": "message_start", "message": {"model": "claude-fable-5",
                "usage": {"input_tokens": 20, "output_tokens": 0}}}),
            json!({"type": "message_delta", "delta": {"stop_reason": "refusal"},
                "usage": {"output_tokens": 0}}),
            json!({"type": "message_stop"}),
        ]);
        check(
            settled_stream(
                &AwsBedrock,
                Operation::StreamGenerateContent,
                Dialect::Claude,
                &refused,
            ),
            json!({"tokens": {"input": 20, "output": 0}, "completeness": "Complete",
                "attempts": [{"model": "claude-fable-5", "billable": false, "output": 0}]}),
            same,
        );
    }

    #[test]
    fn chat_agrees() {
        let chat = br#"{"usage":{"prompt_tokens":11,"completion_tokens":4,"prompt_tokens_details":{"cached_tokens":5}}}"#;
        check(
            settled(
                &AwsBedrock,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                chat,
            ),
            json!({"tokens": {"input": 6, "output": 4, "cached": 5},
                "completeness": "Complete"}),
            same,
        );
    }
}

// ------------------------------------------------------------- workbuddy

/// WorkBuddy's chat was `openai_wire`'s reading; its image reply was broken
/// (the reader saw the vendor envelope) and settles from the unwrapped
/// reply now, which its own tests shape.
#[cfg(feature = "workbuddy")]
mod workbuddy {
    use super::*;
    use gproxy_channel::channels::workbuddy::WorkBuddy;

    #[test]
    fn chat_bodies_and_streams_are_equal() {
        check(
            settled(
                &WorkBuddy,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                &body(
                    &json!({"usage": {"prompt_tokens": 500, "completion_tokens": 8,
                    "prompt_tokens_details": {"cached_tokens": 480}}}),
                ),
            ),
            json!({"tokens": {"input": 20, "output": 8, "cached": 480},
                "completeness": "Complete"}),
            same,
        );
        let wire = b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: {\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n";
        check(
            settled_stream(
                &WorkBuddy,
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                wire,
            ),
            json!({"tokens": {"input": 12, "output": 3}, "completeness": "Complete"}),
            same,
        );
    }

    #[test]
    fn an_unwrapped_image_reply_counts_its_images_alike() {
        check(
            settled(
                &WorkBuddy,
                Operation::CreateImage,
                Dialect::OpenAi,
                br#"{"data":[{"b64_json":"A"},{"b64_json":"B"}]}"#,
            ),
            json!({"tokens": {}, "metrics": {"image_outputs": "2"},
                "completeness": "Complete"}),
            same,
        );
    }
}
