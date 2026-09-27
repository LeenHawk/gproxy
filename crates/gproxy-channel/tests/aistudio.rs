#![cfg(feature = "aistudio")]

use gproxy_channel::{
    BaseChannel, ChannelError,
    channel::{
        CredentialView, PrepareContext, ProviderView, ResponseView, UsageContext, UsageFrame,
        UsageStreamContext, UsageStreamEnd, UsageTransport,
    },
    channels::aistudio::Aistudio,
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest,
    connection::{Bytes, StreamFraming},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel: "aistudio",
        base_url,
        config,
    }
}

fn credential(secret: &Value) -> CredentialView<'_> {
    CredentialView {
        id: "c",
        provider_id: "p",
        auth_kind: "api_key",
        secret,
        metadata: &Value::Null,
        version: 0,
        expires_at_ms: None,
    }
}

/// A client request carrying its own credential in both places, hop-by-hop
/// noise, a resumable-upload header the channel must keep and one it need not.
fn request(path: &str, query: Option<&str>, body: &[u8]) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("x-goog-api-key", HeaderValue::from_static("client-key"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("content-length", HeaderValue::from_static("2"));
    headers.insert(
        "x-goog-upload-protocol",
        HeaderValue::from_static("resumable"),
    );
    headers.insert("x-client-only", HeaderValue::from_static("noise"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: query.map(str::to_owned),
        headers,
        body: HttpBody::Bytes(Bytes::copy_from_slice(body)),
    }
}

fn prepare<'a>(
    config: &'a Value,
    base_url: Option<&'a str>,
    secret: &'a Value,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
    endpoint_override: Option<&'a str>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    Aistudio.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })
}

fn shaped(request: http::Request<HttpBody>) -> Value {
    let HttpBody::Bytes(bytes) = request.into_body() else {
        panic!("buffered");
    };
    serde_json::from_slice(&bytes).unwrap()
}

// ------------------------------------------------------------------ routing

#[test]
fn authenticates_each_surface_with_the_header_that_surface_wants() {
    let config = json!({"headers": {"x-static": "yes"}});
    let secret = json!({"api_key": "  AIza-upstream  "});
    let gemini = prepare(
        &config,
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-3:streamGenerateContent",
            Some("alt=sse&key=leak&pageToken=next"),
            br#"{"contents":[]}"#,
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        gemini.uri(),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3:streamGenerateContent?alt=sse&pageToken=next"
    );
    let headers = gemini.headers();
    assert_eq!(headers["x-goog-api-key"], "AIza-upstream", "trimmed");
    assert!(
        headers.get("authorization").is_none(),
        "the native surface takes no bearer token"
    );
    assert_eq!(headers["x-goog-upload-protocol"], "resumable");
    assert_eq!(headers["x-static"], "yes");
    assert!(headers.get("host").is_none());
    assert!(headers.get("content-length").is_none());

    let chat = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None, br#"{"model":"gemini-3"}"#),
        None,
    )
    .unwrap();
    assert_eq!(
        chat.uri(),
        "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        "the compatibility layer hangs off its own prefix"
    );
    assert_eq!(
        chat.headers()["authorization"],
        "Bearer AIza-upstream",
        "the compatibility layer takes a bearer token"
    );
    assert!(
        chat.headers().get("x-goog-api-key").is_none(),
        "and the client's own key never survives either way"
    );

    let route = |operation, dialect, path| {
        prepare(
            &json!({}),
            Some("https://relay.example/gemini/"),
            &secret,
            operation,
            dialect,
            request(path, None, b"{}"),
            None,
        )
        .unwrap()
        .uri()
        .to_string()
    };
    assert_eq!(
        route(Operation::CreateVideo, Dialect::Gemini, "/v1/videos"),
        "https://relay.example/gemini/v1beta/openai/videos",
        "AI Studio exposes video jobs only on the compatibility layer"
    );
    assert_eq!(
        route(
            Operation::ListFiles,
            Dialect::Gemini,
            "v1beta/files?pageSize=10"
        ),
        "https://relay.example/gemini/v1beta/files?pageSize=10",
        "a relative client path is still rooted"
    );
    assert_eq!(
        route(Operation::ListModels, Dialect::OpenAi, "/v1/models"),
        "https://relay.example/gemini/v1beta/openai/models"
    );

    let overridden = prepare(
        &json!({}),
        Some("https://unused.example"),
        &secret,
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/x:generateContent", None, b"{}"),
        Some("https://exact.example/gen?fixed=1"),
    )
    .unwrap();
    assert_eq!(overridden.uri(), "https://exact.example/gen?fixed=1");

    let view = provider(&config, None);
    assert_eq!(
        Aistudio.native_dialects(view, Operation::GenerateContent),
        [Dialect::Gemini, Dialect::OpenAiChat]
    );
    assert_eq!(
        Aistudio.native_dialects(view, Operation::CountTokens),
        [Dialect::Gemini]
    );
    assert_eq!(
        Aistudio.native_dialects(view, Operation::ListModels),
        [Dialect::Gemini, Dialect::OpenAi]
    );
    assert!(
        Aistudio
            .native_dialects(view, Operation::ConnectRealtime)
            .is_empty(),
        "the Live API is not this channel's surface"
    );
    assert_eq!(Aistudio.descriptor().display_name, "Google AI Studio");
    assert!(
        Aistudio.default_connection().is_none(),
        "a plain HTTPS API has no client identity to impersonate"
    );

    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &json!({"token": "wrong-field"}),
            Operation::GenerateContent,
            Dialect::Gemini,
            request("/v1beta/models/x:generateContent", None, b"{}"),
            None
        ),
        Err(ChannelError::InvalidCredential)
    ));
    assert!(matches!(
        prepare(
            &json!({"allowed_headers": ["not a header"]}),
            None,
            &secret,
            Operation::GenerateContent,
            Dialect::Gemini,
            request("/v1beta/models/x:generateContent", None, b"{}"),
            None
        ),
        Err(ChannelError::InvalidConfig(_))
    ));
}

#[test]
fn every_body_is_forwarded_as_the_client_wrote_it() {
    let secret = json!({"api_key": "k"});
    let chat = shaped(
        prepare(
            &json!({}),
            None,
            &secret,
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            request(
                "/v1/chat/completions",
                None,
                br#"{"model":"gemini-3","stream":true}"#,
            ),
            None,
        )
        .unwrap(),
    );
    assert!(
        chat.get("stream_options").is_none(),
        "core asks a Chat stream for its usage; the channel forwards the body"
    );

    let native = shaped(
        prepare(
            &json!({}),
            None,
            &secret,
            Operation::StreamGenerateContent,
            Dialect::Gemini,
            request(
                "/v1beta/models/gemini-3:streamGenerateContent",
                None,
                br#"{"contents":[{"parts":[{"text":"hi"}]}]}"#,
            ),
            None,
        )
        .unwrap(),
    );
    assert_eq!(
        native,
        json!({"contents": [{"parts": [{"text": "hi"}]}]}),
        "a Gemini body is forwarded as the client wrote it"
    );
}

// -------------------------------------------------------------------- usage

const METADATA: &str = r#"{
    "candidates": [{"finishReason": "STOP"}],
    "usageMetadata": {
        "promptTokenCount": 100,
        "cachedContentTokenCount": 40,
        "candidatesTokenCount": 50,
        "thoughtsTokenCount": 10,
        "totalTokenCount": 160,
        "candidatesTokensDetails": [
            {"modality": "TEXT", "tokenCount": 30},
            {"modality": "IMAGE", "tokenCount": 15},
            {"modality": "AUDIO", "tokenCount": 5}
        ]
    }
}"#;

#[test]
fn splits_the_produced_modalities_out_of_the_candidate_count() {
    let extractor = Aistudio.usage_extractor().expect("declared");
    let mut headers = HeaderMap::new();
    headers.insert("x-gemini-service-tier", "flex".parse().unwrap());
    let usage = extractor
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::Gemini,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: METADATA.as_bytes(),
            },
        })
        .unwrap()
        .expect("reported");
    assert_eq!(
        usage.tokens.input_tokens,
        Some(60),
        "100 prompt tokens less the 40 read from the cache"
    );
    assert_eq!(
        usage.tokens.output_tokens,
        Some(40),
        "50 candidates less 15 image and 5 audio, plus 10 thinking"
    );
    assert_eq!(usage.tokens.cached_input_tokens, Some(40));
    assert_eq!(usage.tokens.reasoning_tokens, Some(10));
    assert_eq!(usage.metrics["image_output_tokens"], Decimal::from(15));
    assert_eq!(usage.metrics["audio_output_tokens"], Decimal::from(5));
    assert_eq!(usage.actual_service_tier.as_deref(), Some("flex"));

    let empty = HeaderMap::new();
    let embedding = extractor
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::CreateEmbedding,
                dialect: Dialect::Gemini,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &empty,
                body: br#"{"embedding":{"values":[1.0]},"usageMetadata":{"promptTokenCount":8}}"#,
            },
        })
        .unwrap()
        .expect("reported");
    assert_eq!(embedding.tokens.input_tokens, Some(8));
    assert_eq!(embedding.tokens.output_tokens, None);

    assert!(
        extractor
            .extract(UsageContext {
                operation: OperationKey {
                    operation: Operation::GenerateContent,
                    dialect: Dialect::Gemini,
                },
                request_body: None,
                response: ResponseView {
                    status: StatusCode::OK,
                    headers: &empty,
                    body: br#"{"candidates":[]}"#,
                },
            })
            .unwrap()
            .is_none(),
        "a reply without usageMetadata reports none, not zero"
    );

    // The compatibility layer's own shape, on the same channel.
    let chat = extractor
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &empty,
                body: br#"{"usage":{"prompt_tokens":7,"completion_tokens":2}}"#,
            },
        })
        .unwrap()
        .expect("reported");
    assert_eq!(chat.tokens.input_tokens, Some(7));
}

#[test]
fn watches_both_stream_framings_and_takes_the_last_complete_reading() {
    let headers = HeaderMap::new();
    let stream = Aistudio.usage_stream().expect("declared");
    let observe = |dialect, framing, chunks: &[&[u8]]| {
        let mut observer = stream
            .start(UsageStreamContext {
                operation: OperationKey {
                    operation: Operation::StreamGenerateContent,
                    dialect,
                },
                request_body: None,
                status: StatusCode::OK,
                headers: &headers,
                transport: UsageTransport::Http { framing },
            })
            .unwrap();
        for chunk in chunks {
            observer.observe(UsageFrame::HttpChunk(chunk)).unwrap();
        }
        observer.finish(UsageStreamEnd::Complete).unwrap()
    };

    // An early record carries a prompt-only reading, which is not a result.
    let partial = br#"{"usageMetadata":{"promptTokenCount":100}}"#;
    let final_record = br#"{"usageMetadata":{"promptTokenCount":100,"candidatesTokenCount":50,"thoughtsTokenCount":10}}"#;

    let sse = observe(
        Dialect::Gemini,
        Some(StreamFraming::Sse),
        &[b"data: ", partial, b"\n\ndata: ", final_record, b"\n\n"],
    )
    .expect("the last complete record wins");
    assert_eq!(
        sse.tokens.input_tokens,
        Some(100),
        "nothing was cached here"
    );
    assert_eq!(sse.tokens.output_tokens, Some(60));

    let array = observe(
        Dialect::Gemini,
        Some(StreamFraming::JsonArray),
        &[b"[", partial, b",", final_record, b"]"],
    )
    .expect("without alt=sse the records arrive as one array");
    assert_eq!(array.tokens.output_tokens, Some(60));

    assert!(
        observe(
            Dialect::Gemini,
            Some(StreamFraming::Sse),
            &[b"data: ", partial, b"\n\n"]
        )
        .is_none(),
        "a prompt-only reading would understate the call"
    );

    let chat = observe(
        Dialect::OpenAiChat,
        Some(StreamFraming::Sse),
        &[b"data: {\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4}}\n\n"],
    )
    .expect("the compatibility layer reports OpenAI's shape");
    assert_eq!(chat.tokens.input_tokens, Some(9));

    assert!(
        stream
            .start(UsageStreamContext {
                operation: OperationKey {
                    operation: Operation::StreamGenerateContent,
                    dialect: Dialect::Gemini,
                },
                request_body: None,
                status: StatusCode::OK,
                headers: &headers,
                transport: UsageTransport::WebSocket,
            })
            .is_err(),
        "AI Studio serves no socket here"
    );
}
