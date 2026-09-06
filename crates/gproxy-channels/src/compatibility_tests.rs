use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, StreamCtx, StreamEnd, UsageCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, StreamFraming, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

#[test]
fn native_openai_chat_preserves_its_wire_and_metering_on_vendor_channels() {
    for (channel, expected_path, expected_model, auth) in [
        (
            &crate::ClaudeApiChannel as &dyn Channel,
            "/v1/chat/completions",
            "claude-sonnet-4-6",
            "x-api-key",
        ),
        (
            &crate::AiStudioChannel,
            "/v1beta/openai/chat/completions",
            "claude-sonnet-4-6",
            "authorization",
        ),
        (
            &crate::VertexChannel,
            "/v1beta1/projects/project/locations/us-central1/endpoints/openapi/chat/completions",
            "google/claude-sonnet-4-6",
            "authorization",
        ),
    ] {
        for stream in [false, true] {
            let key = OperationKey::content(
                if stream {
                    Operation::StreamGenerateContent
                } else {
                    Operation::GenerateContent
                },
                ContentGenerationKind::OpenAiChat,
            );
            assert_eq!(channel.select_support(key, &json!({})).unwrap().target, key);
            let body = Bytes::from_static(br#"{"model":"alias","temperature":0.7,"messages":[{"role":"user","content":"hello"}]}"#);
            let secret = json!({"api_key":"test-key","access_token":"test-token","project_id":"project","location":"us-central1"});
            let prepared = channel
                .prepare(PrepareCtx {
                    session_id: None,
                    key,
                    stream,
                    method: &Method::POST,
                    path: "/v1/chat/completions",
                    query: None,
                    headers: &HeaderMap::new(),
                    body: &body,
                    upstream_model: "claude-sonnet-4-6",
                    provider_settings: &json!({}),
                    secret: &secret,
                })
                .unwrap();
            assert_eq!(prepared.request.uri().path(), expected_path);
            assert!(prepared.request.headers().contains_key(auth));
            let value: Value = serde_json::from_slice(prepared.request.body()).unwrap();
            assert_eq!(value["model"], expected_model);
            assert_eq!(value["messages"][0]["content"], "hello");
            assert_eq!(value["temperature"], 0.7);
            assert!(value.get("contents").is_none());
            let headers = HeaderMap::new();
            if stream {
                assert_eq!(value["stream_options"]["include_usage"], true);
                let mut decoder = channel
                    .stream_decoder(StreamCtx {
                        key,
                        framing: StreamFraming::Sse,
                        request_body: prepared.request.body(),
                        response_headers: &headers,
                    })
                    .unwrap();
                let sse = Bytes::from_static(b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":13,\"completion_tokens\":8,\"total_tokens\":21}}\n\ndata: [DONE]\n\n");
                let _ = decoder.push(sse).unwrap();
                let usage = decoder.finish(StreamEnd::Complete).unwrap().usage.unwrap();
                assert_eq!((usage.input_tokens, usage.output_tokens), (13, 8));
            } else {
                let usage = channel.extract_usage(UsageCtx {key, request_body: &body, response_headers: &headers, response_body: br#"{"usage":{"prompt_tokens":13,"completion_tokens":8,"total_tokens":21}}"#}).unwrap();
                assert_eq!((usage.input_tokens, usage.output_tokens), (13, 8));
            }
        }
    }
}

#[test]
fn openai_catalog_requests_use_vendor_endpoints_and_auth() {
    for (channel, prefix, auth) in [
        (
            &crate::ClaudeApiChannel as &dyn Channel,
            "/v1/models",
            "x-api-key",
        ),
        (
            &crate::AiStudioChannel,
            "/v1beta/openai/models",
            "authorization",
        ),
    ] {
        for operation in [Operation::ListModels, Operation::GetModel] {
            let key = OperationKey::family(operation, WireFamily::OpenAi);
            let (method, path) = gproxy_protocol::request_target(key, "alias").unwrap();
            let prepared = channel
                .prepare(PrepareCtx {
                    session_id: None,
                    key,
                    stream: false,
                    method: &method,
                    path: &path,
                    query: None,
                    headers: &HeaderMap::new(),
                    body: &Bytes::new(),
                    upstream_model: "actual",
                    provider_settings: &json!({}),
                    secret: &json!({"api_key":"test-key"}),
                })
                .unwrap();
            let expected = if operation == Operation::ListModels {
                prefix.into()
            } else {
                format!("{prefix}/actual")
            };
            assert_eq!(prepared.request.uri().path(), expected);
            assert!(prepared.request.headers().contains_key(auth));
        }
    }
}

#[test]
fn realtime_queries_use_selected_models_and_native_websocket_endpoints() {
    for (channel, secret) in [
        (
            &crate::OpenAiChannel as &dyn Channel,
            json!({"api_key":"test-key"}),
        ),
        (&crate::CodexChannel, json!({"access_token":"test-token"})),
    ] {
        let key = OperationKey::family(Operation::ConnectRealtime, WireFamily::OpenAi);
        for (query, expected) in [
            ("model=alias&key=client", "model=actual%2Fmodel"),
            ("call_id=rtc_test&model=alias", "call_id=rtc_test"),
        ] {
            let prepared = channel
                .prepare(PrepareCtx {
                    session_id: None,
                    key,
                    stream: false,
                    method: &Method::GET,
                    path: "/v1/realtime",
                    query: Some(query),
                    headers: &HeaderMap::new(),
                    body: &Bytes::new(),
                    upstream_model: "actual/model",
                    provider_settings: &json!({}),
                    secret: &secret,
                })
                .unwrap();
            assert!(prepared.websocket);
            assert_eq!(
                prepared.request.uri().to_string(),
                format!("wss://api.openai.com/v1/realtime?{expected}")
            );
            assert!(!prepared.request.headers().contains_key("openai-beta"));
        }
    }
}
