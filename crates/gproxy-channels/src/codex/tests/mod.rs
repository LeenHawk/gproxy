use std::future::Future;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, Channel, ClientProfile, PrepareCtx, ResponseShapeCtx, SimpleHttp, StreamCtx,
    StreamDecoder, StreamEnd, SurfaceRequest, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::CodexChannel;

mod auth;
mod cache;
mod memory;
mod quota;
mod realtime;

const RESPONSES: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::OpenAiResponses,
);
#[test]
fn descriptor_declares_every_current_transform_pair() {
    let supports = CodexChannel.descriptor().supports;
    assert_eq!(supports.len(), 22);
    for (source, target) in [
        (
            OperationKey::family(Operation::ListModels, gproxy_protocol::WireFamily::Claude),
            OperationKey::family(Operation::ListModels, gproxy_protocol::WireFamily::OpenAi),
        ),
        (
            OperationKey::family(Operation::GetModel, gproxy_protocol::WireFamily::Claude),
            OperationKey::family(Operation::GetModel, gproxy_protocol::WireFamily::OpenAi),
        ),
        (
            OperationKey::content(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            RESPONSES,
        ),
        (
            OperationKey::content(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiChat,
            ),
            RESPONSES,
        ),
        (
            OperationKey::content(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiChat,
            ),
            RESPONSES,
        ),
    ] {
        assert!(
            supports
                .iter()
                .any(|support| support.source == source && support.target == target),
            "missing {source:?} -> {target:?}"
        );
    }
}

#[test]
fn prepare_applies_codex_endpoint_headers_profile_and_typed_shape() {
    let secret = json!({"access_token":"token", "account_id":"account"});
    let settings = json!({});
    let mut headers = HeaderMap::new();
    headers.insert("session-id", "session-1".parse().unwrap());
    headers.insert("x-codex-turn-state", "turn-1".parse().unwrap());
    let body = Bytes::from(
        json!({
            "model":"route",
            "input":"hello",
            "max_output_tokens":100,
            "stream_options":{"include_obfuscation":false,"future":1},
            "future_request":true
        })
        .to_string(),
    );
    let prepared = CodexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: RESPONSES,
            stream: true,
            method: &Method::POST,
            path: "/v1/responses",
            query: Some("key=downstream&foo=1"),
            headers: &headers,
            body: &body,
            upstream_model: "gpt-5.4",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        prepared.request.uri(),
        "https://chatgpt.com/backend-api/codex/responses?foo=1"
    );
    assert_eq!(prepared.request.headers()["authorization"], "Bearer token");
    assert_eq!(prepared.request.headers()["chatgpt-account-id"], "account");
    assert_eq!(prepared.request.headers()["originator"], "codex_cli_rs");
    assert_eq!(prepared.request.headers()["session-id"], "session-1");
    assert_eq!(prepared.profile, Some(&super::profile::CLIENT_PROFILE));
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["model"], "gpt-5.4");
    assert_eq!(shaped["stream"], true);
    assert_eq!(shaped["store"], false);
    assert!(shaped.get("max_output_tokens").is_none());
    assert!(shaped.get("instructions").is_none());
    assert_eq!(shaped["stream_options"]["future"], 1);
    assert_eq!(shaped["future_request"], true);
}

#[test]
fn model_catalog_shapes_to_public_openai_models() {
    let body = Bytes::from_static(
        br#"{"models":[{"slug":"gpt-5.4-codex","context_window":272000,"max_context_window":872000,"supported_reasoning_levels":["high"],"future_model":7}],"future_catalog_field":true}"#,
    );
    let shaped = CodexChannel
        .shape_response(ResponseShapeCtx {
            key: OperationKey::family(Operation::ListModels, gproxy_protocol::WireFamily::OpenAi),
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &body,
        })
        .unwrap();
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["object"], "list");
    assert_eq!(value["data"][0]["id"], "gpt-5.4-codex");
    assert_eq!(value["data"][0]["context_window"], 872000);
    assert_eq!(value["data"][0]["thinking_supported"], true);
    assert_eq!(value["data"][0]["future_model"], 7);
    assert_eq!(value["future_catalog_field"], true);
}

#[test]
fn image_streaming_is_rejected_before_buffered_backend_send() {
    let body = Bytes::from(json!({"prompt":"draw", "model":"route", "stream":true}).to_string());
    let error = CodexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::CreateImage, gproxy_protocol::WireFamily::OpenAi),
            stream: true,
            method: &Method::POST,
            path: "/v1/images/generations",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "gpt-image-1",
            provider_settings: &json!({}),
            secret: &json!({"access_token":"token"}),
        })
        .err()
        .unwrap();
    assert!(error.to_string().contains("image streaming"));
}

#[test]
fn multipart_image_edit_is_binary_safe_and_preserves_future_fields() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        "multipart/form-data; boundary=test".parse().unwrap(),
    );
    let body = Bytes::from_static(
        b"--test\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nedit\r\n--test\r\nContent-Disposition: form-data; name=\"quality\"\r\n\r\nhigh\r\n--test\r\nContent-Disposition: form-data; name=\"future_mode\"\r\n\r\nkept\r\n--test\r\nContent-Disposition: form-data; name=\"image[]\"; filename=\"x.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n\x00\xff--test\r\n\r\n--test--\r\n",
    );
    let prepared = CodexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::EditImage, gproxy_protocol::WireFamily::OpenAi),
            stream: false,
            method: &Method::POST,
            path: "/v1/images/edits",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "gpt-image-1",
            provider_settings: &json!({}),
            secret: &json!({"access_token":"token"}),
        })
        .unwrap();
    let value: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(value["prompt"], "edit");
    assert_eq!(value["future_mode"], "kept");
    assert_eq!(
        value["images"][0]["image_url"],
        "data:application/octet-stream;base64,AP8tLXRlc3QNCg=="
    );
    assert_eq!(
        prepared.request.headers()[http::header::CONTENT_TYPE],
        "application/json"
    );
}

#[test]
fn typed_tool_and_image_shaping_preserves_only_allowed_extensions() {
    let request = Bytes::from(
        json!({
            "model":"route",
            "input":[
                {"type":"message","role":"system","content":[{"type":"input_text","text":"policy"}]},
                {"type":"reasoning","id":"r1","summary":[],"status":"completed","future_item":1},
                {"type":"local_shell_call_output","id":"out1","call_id":"call-real","output":"ok","status":"completed"}
            ],
            "tools":[
                {"type":"shell"},
                {"type":"apply_patch"},
                {"type":"tool_search","execution":"client"}
            ],
            "future_request":true
        })
        .to_string(),
    );
    let shaped = super::shape::request(
        Operation::StreamGenerateContent,
        &HeaderMap::new(),
        &request,
        "gpt-5.4",
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["instructions"], "policy");
    assert!(value["input"][0].get("status").is_none());
    assert_eq!(value["input"][0]["future_item"], 1);
    assert_eq!(value["input"][1]["call_id"], "call-real");
    assert_eq!(value["tools"][0]["type"], "function");
    assert_eq!(value["tools"][0]["name"], "shell_command");
    assert_eq!(value["tools"][1]["type"], "custom");
    assert_eq!(value["tools"][1]["name"], "apply_patch");
    assert!(value["tools"][2].get("parameters").is_some());
    assert_eq!(value["future_request"], true);

    let image = Bytes::from(
        json!({
            "prompt":"draw",
            "model":"route",
            "moderation":"low",
            "future_image_option":{"x":1}
        })
        .to_string(),
    );
    let shaped = super::shape::request(
        Operation::CreateImage,
        &HeaderMap::new(),
        &image,
        "gpt-image-1",
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert!(value.get("moderation").is_none());
    assert_eq!(value["future_image_option"]["x"], 1);

    let realtime = Bytes::from(
        json!({
            "sdp":"v=offer",
            "model":"route",
            "session":{"type":"realtime","future_session":true},
            "future_call":1
        })
        .to_string(),
    );
    let shaped = super::shape::request(
        Operation::CreateRealtimeCall,
        &HeaderMap::new(),
        &realtime,
        "gpt-realtime",
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert!(value.get("model").is_none());
    assert_eq!(value["session"]["model"], "gpt-realtime");
    assert_eq!(value["session"]["future_session"], true);
    assert_eq!(value["future_call"], 1);
}

#[test]
fn stream_restores_shell_items_patches_terminal_output_and_extracts_usage() {
    let input = concat!(
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"fc1\",\"call_id\":\"c1\",\"name\":\"shell_command\",\"arguments\":\"\"}}\n\n",
        "event: response.function_call_arguments.done\n",
        "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":0,\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}\n\n",
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"fc1\",\"call_id\":\"c1\",\"name\":\"shell_command\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"output_index\":1,\"item_id\":\"m1\",\"content_index\":0,\"delta\":\"hi\"}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"created_at\":1,\"object\":\"response\",\"output\":[],\"output_tokens_details\":{},\"usage\":{\"input_tokens\":2,\"output_tokens\":1,\"total_tokens\":3,\"output_tokens_details\":{\"reasoning_tokens\":0}}}}\n\n"
    );
    let mut decoder = super::sse::CodexSseDecoder::for_operation(StreamCtx {
        key: RESPONSES,
        framing: gproxy_protocol::StreamFraming::Sse,
        request_body: &Bytes::new(),
        response_headers: &HeaderMap::new(),
    })
    .unwrap();
    let frames = decoder.push(Bytes::from(input)).unwrap();
    let text = frames
        .iter()
        .map(|frame| String::from_utf8_lossy(&frame.0))
        .collect::<String>();
    assert!(text.contains("shell_call"));
    assert!(text.contains("commands"));
    assert!(text.contains("\"text\":\"hi\""));
    let tail = decoder.finish(StreamEnd::Complete).unwrap();
    assert_eq!(tail.usage.unwrap().input_tokens, 2);
}

#[test]
fn sparse_stream_repairs_tool_lifecycle_before_exact_terminal() {
    let input = concat!(
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"f1\",\"call_id\":\"c1\",\"name\":\"f\",\"arguments\":\"\"}}\n\n",
        "event: response.function_call_arguments.delta\n",
        "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":0,\"item_id\":\"f1\",\"delta\":\"{}\"}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"custom_tool_call\",\"id\":\"t1\",\"call_id\":\"c2\",\"name\":\"custom\",\"input\":\"\"}}\n\n",
        "event: response.custom_tool_call_input.delta\n",
        "data: {\"type\":\"response.custom_tool_call_input.delta\",\"output_index\":1,\"item_id\":\"t1\",\"delta\":\"patch\"}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"output_index\":2,\"item_id\":\"m1\",\"content_index\":0,\"delta\":\"done\"}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"object\":\"response\",\"created_at\":1,\"status\":\"completed\",\"output\":[]}}\n\n"
    );
    let mut decoder = super::sse::CodexSseDecoder::for_operation(StreamCtx {
        key: RESPONSES,
        framing: gproxy_protocol::StreamFraming::Sse,
        request_body: &Bytes::new(),
        response_headers: &HeaderMap::new(),
    })
    .unwrap();
    let mut frames = Vec::new();
    for chunk in input.as_bytes().chunks(7) {
        frames.extend(decoder.push(Bytes::copy_from_slice(chunk)).unwrap());
    }
    let tail = decoder.finish(StreamEnd::Complete).unwrap();
    frames.extend(tail.frames);
    let text = frames
        .iter()
        .map(|frame| String::from_utf8_lossy(&frame.0))
        .collect::<String>();
    assert!(text.contains("response.function_call_arguments.done"));
    assert!(text.contains("response.custom_tool_call_input.done"));
    assert!(text.contains("response.output_item.done"));
    assert!(text.contains("response.completed"));
    assert!(text.contains("\"text\":\"done\""));
}

#[test]
fn usage_keeps_actual_tier_and_image_dimensions() {
    let response = Bytes::from_static(
        br#"{"id":"r1","created_at":1,"object":"response","output":[],"service_tier":"priority","usage":{"input_tokens":2,"input_tokens_details":{"cache_write_tokens":1},"output_tokens":1,"total_tokens":3,"output_tokens_details":{"reasoning_tokens":0}}}"#,
    );
    let usage = super::usage::from_body(UsageCtx {
        key: RESPONSES,
        request_body: &Bytes::new(),
        response_headers: &HeaderMap::new(),
        response_body: &response,
    })
    .unwrap();
    assert_eq!(usage.input_tokens, 1);
    assert_eq!(usage.dimensions["service_tier"], "priority");
    assert_eq!(usage.metrics["cache_creation_30m_tokens"], 1.into());

    let image = Bytes::from_static(
        br#"{"created":1,"data":[],"size":"1024x1024","quality":"high","usage":{"input_tokens":1,"input_tokens_details":{"image_tokens":0,"text_tokens":1},"output_tokens":2,"total_tokens":3,"output_tokens_details":{"image_tokens":2,"text_tokens":0}}}"#,
    );
    let usage = super::usage::from_body(UsageCtx {
        key: OperationKey::family(Operation::CreateImage, gproxy_protocol::WireFamily::OpenAi),
        request_body: &Bytes::new(),
        response_headers: &HeaderMap::new(),
        response_body: &image,
    })
    .unwrap();
    assert_eq!(usage.dimensions["size"], "1024x1024");
    assert_eq!(usage.dimensions["quality"], "high");
}

#[test]
fn refresh_uses_codex_client_profile() {
    let http = MockHttp {
        captured_profile: Mutex::new(false),
    };
    let secret = json!({"access_token":"old", "refresh_token":"refresh"});
    let rotated = ready(super::auth::refresh(&secret, &http)).unwrap();
    assert_eq!(rotated["access_token"], "new");
    assert!(*http.captured_profile.lock().unwrap());
}

#[test]
fn surface_prepare_uses_backend_base_and_preserves_remote_bearer() {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer remote-token".parse().unwrap());
    headers.insert("cookie", "secret=1".parse().unwrap());
    headers.insert("x-forwarded-for", "127.0.0.1".parse().unwrap());
    let prepared = super::prepare::surface(
        &SurfaceRequest {
            label: "remote_control_ws",
            key: None,
            stream: false,
            method: Method::GET,
            upstream_path: "/wham/remote/control/server".into(),
            query: Some("key=remote-token&server=1".into()),
            headers,
            body: Bytes::new(),
            credential: None,
        },
        true,
        &json!({}),
        &json!({"access_token":"oauth-token"}),
    )
    .unwrap();
    assert_eq!(
        prepared.request.uri(),
        "wss://chatgpt.com/backend-api/wham/remote/control/server?key=remote-token&server=1"
    );
    assert_eq!(
        prepared.request.headers()["authorization"],
        "Bearer remote-token"
    );
    assert_eq!(prepared.profile, Some(&super::profile::CLIENT_PROFILE));
    assert!(prepared.request.headers().get("cookie").is_none());
    assert!(prepared.request.headers().get("x-forwarded-for").is_none());

    let ordinary = super::prepare::surface(
        &SurfaceRequest {
            label: "tasks",
            key: None,
            stream: false,
            method: Method::GET,
            upstream_path: "/wham/tasks/list".into(),
            query: Some("key=downstream&limit=10".into()),
            headers: HeaderMap::from_iter([
                (http::header::COOKIE, "secret=1".parse().unwrap()),
                (
                    http::header::AUTHORIZATION,
                    "Bearer downstream".parse().unwrap(),
                ),
            ]),
            body: Bytes::new(),
            credential: None,
        },
        false,
        &json!({}),
        &json!({"access_token":"oauth-token"}),
    )
    .unwrap();
    assert_eq!(
        ordinary.request.uri(),
        "https://chatgpt.com/backend-api/wham/tasks/list?limit=10"
    );
    assert_eq!(
        ordinary.request.headers()["authorization"],
        "Bearer oauth-token"
    );
    assert!(ordinary.request.headers().get("cookie").is_none());
}

struct MockHttp {
    captured_profile: Mutex<bool>,
}

impl SimpleHttp for MockHttp {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, gproxy_channel_api::ChannelError>> {
        *self.captured_profile.lock().unwrap() =
            request.extensions().get::<ClientProfile>() == Some(&super::profile::CLIENT_PROFILE);
        Box::pin(async {
            Ok(http::Response::new(Bytes::from_static(
                br#"{"access_token":"new","expires_in":3600}"#,
            )))
        })
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}
