use bytes::Bytes;
use gproxy_channel_api::{
    Channel, Disposition, PrepareCtx, ResponseView, StreamCtx, StreamEnd, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::OpenAiChannel;

const CHAT: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::OpenAiChat,
);
const RESPONSES: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::OpenAiResponses,
);

const CACHE_MAGIC: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

#[test]
fn descriptor_and_disposition_are_explicit() {
    let descriptor = OpenAiChannel.descriptor();
    assert_eq!(
        (descriptor.id, descriptor.display_name),
        ("openai", "OpenAI")
    );
    assert_eq!(
        descriptor
            .supports
            .iter()
            .filter(|support| support.source == support.target)
            .count(),
        30
    );
    for key in [
        OperationKey::family(Operation::ListModels, WireFamily::OpenAi),
        OperationKey::family(Operation::ExtendVideo, WireFamily::OpenAi),
        CHAT,
        RESPONSES,
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponsesWebSocket,
        ),
    ] {
        assert!(
            descriptor
                .supports
                .iter()
                .any(|support| support.source == key)
        );
    }
    for operation in [
        Operation::CountTokens,
        Operation::Rerank,
        Operation::WebSearch,
        Operation::CreateRealtimeCall,
    ] {
        let key = OperationKey::family(operation, WireFamily::OpenAi);
        assert!(
            !descriptor
                .supports
                .iter()
                .any(|support| support.source == key)
        );
    }

    let headers = HeaderMap::new();
    for (status, expected) in [
        (StatusCode::OK, Disposition::Success),
        (StatusCode::UNAUTHORIZED, Disposition::CredentialDead),
        (StatusCode::PAYMENT_REQUIRED, Disposition::CredentialDead),
        (StatusCode::TOO_MANY_REQUESTS, Disposition::Retryable),
        (StatusCode::BAD_GATEWAY, Disposition::Retryable),
        (StatusCode::BAD_REQUEST, Disposition::Terminal),
    ] {
        assert_eq!(
            OpenAiChannel.classify(ResponseView {
                status,
                headers: &headers,
                body: &[],
            }),
            expected
        );
    }
}

#[test]
fn prepare_sanitizes_and_shapes_json_and_exact_endpoints() {
    let secret = json!({"api_key":"  sk-upstream  "});
    let settings = json!({
        "base_url": "https://unused.example/",
        "endpoints": {
            "openai_chat_completions": "https://relay.example/chat?fixed=1",
            "openai_file_content": "https://media.example/files/{file_id}/content"
        }
    });
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer downstream".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("openai-beta", "feature=v1".parse().unwrap());
    headers.insert("openai-alpha", "drop-me".parse().unwrap());
    headers.insert("cookie", "drop=me".parse().unwrap());
    let body =
        Bytes::from_static(br#"{"model":"route","stream":true,"stream_options":{"other":1}}"#);
    let prepared = OpenAiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: CHAT,
            stream: true,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: Some("purpose=assistants&key=downstream&ignored=x"),
            headers: &headers,
            body: &body,
            upstream_model: "gpt-upstream",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert!(!prepared.websocket);
    assert_eq!(
        prepared.request.uri(),
        "https://relay.example/chat?fixed=1&purpose=assistants"
    );
    assert_eq!(
        prepared.request.headers()["authorization"],
        "Bearer sk-upstream"
    );
    assert_eq!(prepared.request.headers()["openai-beta"], "feature=v1");
    assert!(prepared.request.headers().get("openai-alpha").is_none());
    assert!(prepared.request.headers().get("cookie").is_none());
    let value: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(value["model"], "gpt-upstream");
    assert_eq!(value["stream_options"]["include_usage"], true);
    assert_eq!(value["stream_options"]["other"], 1);

    let empty = Bytes::new();
    let no_headers = HeaderMap::new();
    let file = OpenAiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::RetrieveFileContent, WireFamily::OpenAi),
            stream: false,
            method: &Method::GET,
            path: "/v1/files/file one/content",
            query: None,
            headers: &no_headers,
            body: &empty,
            upstream_model: "",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        file.request.uri(),
        "https://media.example/files/file%20one/content"
    );
}

#[test]
fn prepare_applies_responses_cache_markers_after_existing_breakpoints() {
    let secret = json!({"api_key":"sk"});
    let settings = json!({"enable_openai_magic_cache":true});
    let body = Bytes::from(
        json!({
            "model":"route",
            "instructions":format!("stable policy {CACHE_MAGIC}"),
            "input":[{
                "role":"user",
                "content":[
                    {"type":"input_text","text":"old","prompt_cache_breakpoint":{"mode":"explicit"}},
                    {"type":"input_text","text":format!("new {CACHE_MAGIC}")}
                ]
            }]
        })
        .to_string(),
    );
    let prepared = OpenAiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: RESPONSES,
            stream: true,
            method: &Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "gpt-5.4",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["instructions"], "stable policy ");
    assert_eq!(
        shaped["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert_eq!(
        shaped["input"][1]["content"][1]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert!(!shaped.to_string().contains(CACHE_MAGIC));
}

#[test]
fn prepare_rewrites_binary_safe_multipart_model() {
    let secret = json!({"api_key":"sk"});
    let settings = json!({});
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "multipart/form-data; boundary=x-boundary".parse().unwrap(),
    );
    let body = Bytes::from_static(
        b"--x-boundary\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nroute\r\n--x-boundary\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\n\x00\xff\r\n--x-boundary--\r\n",
    );
    let prepared = OpenAiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::CreateTranscription, WireFamily::OpenAi),
            stream: false,
            method: &Method::POST,
            path: "/v1/audio/transcriptions",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "whisper-upstream",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert!(
        prepared
            .request
            .body()
            .windows(b"whisper-upstream".len())
            .any(|value| value == b"whisper-upstream")
    );
    assert!(
        prepared
            .request
            .body()
            .windows(2)
            .any(|value| value == b"\x00\xff")
    );
}

#[test]
fn buffered_and_fragmented_stream_usage_normalize() {
    let request = Bytes::new();
    let headers = HeaderMap::new();
    let chat = OpenAiChannel
        .extract_usage(UsageCtx {
            key: CHAT,
            request_body: &request,
            response_headers: &headers,
            response_body: br#"{"usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":40,"cache_write_tokens":3},"completion_tokens_details":{"reasoning_tokens":7}}}"#,
        })
        .unwrap();
    assert_eq!((chat.input_tokens, chat.output_tokens), (97, 20));
    assert_eq!(chat.cached_input_tokens, 40);
    assert_eq!(chat.metrics["cache_creation_30m_tokens"], Decimal::from(3));
    assert_eq!(chat.metrics["reasoning_tokens"], Decimal::from(7));

    let transcript = OpenAiChannel
        .extract_usage(UsageCtx {
            key: OperationKey::family(Operation::CreateTranscription, WireFamily::OpenAi),
            request_body: &request,
            response_headers: &headers,
            response_body: br#"{"usage":{"type":"duration","seconds":1.25}}"#,
        })
        .unwrap();
    assert_eq!(transcript.metrics["audio_seconds"], Decimal::new(125, 2));

    let speech_request = Bytes::from_static(br#"{"response_format":"pcm"}"#);
    let speech_body = vec![0_u8; 48_000];
    let speech = OpenAiChannel
        .extract_usage(UsageCtx {
            key: OperationKey::family(Operation::CreateSpeech, WireFamily::OpenAi),
            request_body: &speech_request,
            response_headers: &headers,
            response_body: &speech_body,
        })
        .unwrap();
    assert_eq!(speech.metrics["audio_seconds"], Decimal::ONE);

    let mut speech_stream = OpenAiChannel
        .stream_decoder(StreamCtx {
            key: OperationKey::family(Operation::CreateSpeech, WireFamily::OpenAi),
            framing: gproxy_protocol::StreamFraming::Sse,
            request_body: &speech_request,
            response_headers: &headers,
        })
        .unwrap();
    speech_stream
        .push(Bytes::from_static(
            b"data: {\"type\":\"speech.audio.delta\",\"delta\":\"AAAA\"}\n\n",
        ))
        .unwrap();
    assert_eq!(
        speech_stream
            .finish(StreamEnd::Complete)
            .unwrap()
            .usage
            .unwrap()
            .metrics["audio_seconds"],
        Decimal::from(3_u64) / Decimal::from(48_000_u64)
    );

    let mut decoder = OpenAiChannel
        .stream_decoder(StreamCtx {
            key: RESPONSES,
            framing: gproxy_protocol::StreamFraming::Sse,
            request_body: &request,
            response_headers: &headers,
        })
        .unwrap();
    let chunks = [
        Bytes::from_static(b"event: response.com"),
        Bytes::from_static(
            b"pleted\r\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{",
        ),
        Bytes::from_static(b"\"input_tokens\":9,\"output_tokens\":4}}}\r\n\r\n"),
    ];
    for chunk in chunks {
        let pointer = chunk.as_ptr();
        let frames = decoder.push(chunk).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0.as_ptr(), pointer);
    }
    let tail = decoder.finish(StreamEnd::Complete).unwrap();
    let usage = tail.usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (9, 4));
    assert!(tail.frames.is_empty());
}

#[test]
fn image_stream_redacts_large_media_only_in_the_usage_observer() {
    let key = OperationKey::family(Operation::CreateImage, WireFamily::OpenAi);
    let request = Bytes::new();
    let headers = HeaderMap::new();
    let mut decoder = OpenAiChannel
        .stream_decoder(StreamCtx {
            key,
            framing: gproxy_protocol::StreamFraming::Sse,
            request_body: &request,
            response_headers: &headers,
        })
        .unwrap();
    let event = format!(
        "data: {{\"type\":\"image_generation.completed\",\"b64_json\":\"{}\",\"usage\":{{\"input_tokens\":2,\"output_tokens\":8}}}}\n\n",
        "A".repeat(1024 * 1024 + 1)
    );
    let mut relayed = 0;
    for chunk in event.as_bytes().chunks(8191) {
        let frames = decoder.push(Bytes::copy_from_slice(chunk)).unwrap();
        relayed += frames.into_iter().map(|frame| frame.0.len()).sum::<usize>();
    }
    assert_eq!(relayed, event.len());
    let usage = decoder.finish(StreamEnd::Complete).unwrap().usage.unwrap();
    assert_eq!(usage.input_tokens, 2);
    assert_eq!(usage.output_tokens, 0);
    assert_eq!(usage.metrics["image_output_tokens"], Decimal::from(8));
}
