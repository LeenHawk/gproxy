use std::future::Future;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, Channel, Disposition, PrepareCtx, ResponseView, SimpleHttp, StreamCtx,
    SurfaceRequest, TransportProfile, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::ClaudeCodeChannel;

const MESSAGES: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::ClaudeMessages,
);
const STREAM: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::ClaudeMessages,
);

#[test]
fn descriptor_disposition_and_surface_table_are_explicit() {
    let descriptor = ClaudeCodeChannel.descriptor();
    assert_eq!(
        (descriptor.id, descriptor.display_name),
        ("claudecode", "Claude Code")
    );
    assert_eq!(descriptor.supports.len(), 13);
    assert_eq!(
        descriptor
            .supports
            .iter()
            .filter(|support| support.source == support.target)
            .count(),
        5
    );
    assert_eq!(ClaudeCodeChannel.surfaces().0.len(), 23);

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
            ClaudeCodeChannel.classify(ResponseView {
                status,
                headers: &headers,
                body: &[],
            }),
            expected
        );
    }
}

#[test]
fn prepare_applies_cli_shape_hygiene_cch_and_exact_endpoints() {
    let secret = json!({
        "access_token": " upstream-token ",
        "refresh_token": "refresh",
        "device_id": "device-1",
        "account_uuid": "account-1"
    });
    let settings = json!({
        "base_url": "https://unused.example/",
        "endpoints": {
            "claude_messages": "https://relay.example/messages?fixed=1",
            "claude_get_model": "https://models.example/{model}?fixed=1"
        }
    });
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        "feature-x,context-1m-2025-08-07,oauth-2025-04-20"
            .parse()
            .unwrap(),
    );
    headers.insert("x-claude-code-session-id", "session-1".parse().unwrap());
    headers.insert("authorization", "Bearer downstream".parse().unwrap());
    let body = Bytes::from(
        json!({
            "model": "route-model",
            "speed": "fast",
            "temperature": 0.7,
            "top_p": 0.9,
            "top_k": 40,
            "system": [
                {"type":"text", "text":" policy "},
                {"type":"text", "text":" ", "cache_control":{"type":"ephemeral"}}
            ],
            "messages": [
                {"role":"user", "content":"reply with exactly: ok"},
                {"role":"assistant", "content":"prefix"}
            ]
        })
        .to_string(),
    );
    let prepared = ClaudeCodeChannel
        .prepare(PrepareCtx {
            key: MESSAGES,
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: Some("foo=1&key=downstream"),
            headers: &headers,
            body: &body,
            upstream_model: "claude-opus-4-8",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        prepared.request.uri(),
        "https://relay.example/messages?fixed=1&beta=true&foo=1"
    );
    assert_eq!(
        prepared.request.extensions().get::<TransportProfile>(),
        Some(&TransportProfile::ClaudeCode)
    );
    assert_eq!(
        prepared.request.headers()["authorization"],
        "Bearer upstream-token"
    );
    assert_eq!(prepared.request.headers()["x-app"], "cli");
    assert_eq!(
        prepared.request.headers()["x-claude-code-session-id"],
        "session-1"
    );
    assert_eq!(
        prepared.request.headers()["anthropic-beta"],
        "oauth-2025-04-20,feature-x,fast-mode-2026-02-01"
    );
    assert_eq!(
        prepared.request.headers()["x-stainless-package-version"],
        "0.81.0"
    );
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["model"], "claude-opus-4-8");
    assert!(shaped.get("temperature").is_none());
    assert!(shaped.get("top_p").is_none());
    assert!(shaped.get("top_k").is_none());
    assert_eq!(shaped["messages"][1]["role"], "user");
    assert_eq!(shaped["system"][1]["text"], "policy");
    assert_eq!(shaped["system"][1]["cache_control"]["type"], "ephemeral");
    assert!(
        shaped["system"][0]["text"]
            .as_str()
            .unwrap()
            .contains("x-anthropic-billing-header")
    );
    let ids: Value = serde_json::from_str(shaped["metadata"]["user_id"].as_str().unwrap()).unwrap();
    assert_eq!(ids["device_id"], "device-1");
    assert_eq!(ids["account_uuid"], "account-1");
    assert_eq!(ids["session_id"], "session-1");

    let empty = Bytes::new();
    let get = ClaudeCodeChannel
        .prepare(PrepareCtx {
            key: OperationKey::family(Operation::GetModel, WireFamily::Claude),
            stream: false,
            method: &Method::GET,
            path: "/v1/models/route",
            query: None,
            headers: &HeaderMap::new(),
            body: &empty,
            upstream_model: "claude/model one",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        get.request.uri(),
        "https://models.example/claude%2Fmodel%20one?fixed=1"
    );
}

#[test]
fn count_tokens_and_surface_requests_preserve_their_wire_contracts() {
    let secret = json!({"access_token":"token", "device_id":"device"});
    let settings = json!({});
    let headers = HeaderMap::new();
    let body = Bytes::from_static(
        br#"{"model":"route","speed":"fast","messages":[],"metadata":{"kept":true}}"#,
    );
    let count = ClaudeCodeChannel
        .prepare(PrepareCtx {
            key: OperationKey::family(Operation::CountTokens, WireFamily::Claude),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages/count_tokens",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "claude-sonnet-4-6",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        count.request.uri(),
        "https://api.anthropic.com/v1/messages/count_tokens"
    );
    let count_body: Value = serde_json::from_slice(count.request.body()).unwrap();
    assert_eq!(count_body["metadata"], json!({"kept": true}));
    assert_eq!(
        count.request.headers()["anthropic-beta"],
        "oauth-2025-04-20,fast-mode-2026-02-01"
    );

    let mut resource_headers = HeaderMap::new();
    resource_headers.insert(
        http::header::CONTENT_TYPE,
        "multipart/form-data; boundary=x".parse().unwrap(),
    );
    resource_headers.insert(http::header::ACCEPT, "application/json".parse().unwrap());
    let surface = ClaudeCodeChannel
        .prepare_surface(
            &SurfaceRequest {
                label: "skill-create",
                key: None,
                stream: false,
                method: Method::POST,
                upstream_path: "/v1/skills".into(),
                query: Some("key=downstream&source=custom".into()),
                headers: resource_headers,
                body: Bytes::from_static(b"multipart-body"),
                credential: None,
            },
            false,
            &settings,
            &secret,
        )
        .unwrap();
    assert_eq!(
        surface.request.uri(),
        "https://api.anthropic.com/v1/skills?beta=true&source=custom"
    );
    assert_eq!(
        surface.request.headers()[http::header::CONTENT_TYPE],
        "multipart/form-data; boundary=x"
    );
    assert_eq!(
        surface.request.headers()["anthropic-beta"],
        "oauth-2025-04-20,skills-2025-10-02"
    );
}

#[test]
fn buffered_and_fragmented_stream_usage_merge_claude_fields() {
    let request = Bytes::new();
    let headers = HeaderMap::new();
    let buffered = ClaudeCodeChannel
        .extract_usage(UsageCtx {
            key: MESSAGES,
            request_body: &request,
            response_headers: &headers,
            response_body: br#"{"usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":30,"cache_creation":{"ephemeral_5m_input_tokens":2,"ephemeral_1h_input_tokens":3},"output_tokens_details":{"thinking_tokens":1},"server_tool_use":{"web_search_requests":2,"web_fetch_requests":1}}}"#,
        })
        .unwrap();
    assert_eq!((buffered.input_tokens, buffered.output_tokens), (40, 4));
    assert_eq!(buffered.cached_input_tokens, 30);
    assert_eq!(
        buffered.metrics["cache_creation_5m_tokens"],
        Decimal::from(2)
    );
    assert_eq!(
        buffered.metrics["cache_creation_1h_tokens"],
        Decimal::from(3)
    );
    assert_eq!(buffered.metrics["reasoning_tokens"], Decimal::ONE);
    assert_eq!(buffered.metrics["web_searches"], Decimal::from(2));
    assert_eq!(buffered.metrics["web_fetches"], Decimal::ONE);

    let mut decoder = ClaudeCodeChannel
        .stream_decoder(StreamCtx {
            key: STREAM,
            request_body: &request,
            response_headers: &headers,
        })
        .unwrap();
    let stream = concat!(
        "event: message_start\r\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_read_input_tokens\":10,\"cache_creation\":{\"ephemeral_5m_input_tokens\":0,\"ephemeral_1h_input_tokens\":20}}}}\r\n\r\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":12,\"cache_creation_input_tokens\":20}}\n\n"
    );
    for chunk in stream.as_bytes().chunks(37) {
        let chunk = Bytes::copy_from_slice(chunk);
        let pointer = chunk.as_ptr();
        let frames = decoder.push(chunk).unwrap();
        assert_eq!(frames[0].0.as_ptr(), pointer);
    }
    let usage = decoder.finish().unwrap().usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (35, 12));
    assert_eq!(usage.cached_input_tokens, 10);
    assert!(!usage.metrics.contains_key("cache_creation_5m_tokens"));
    assert_eq!(usage.metrics["cache_creation_1h_tokens"], Decimal::from(20));
}

#[test]
fn refresh_uses_profile_and_preserves_rotating_secret_fields() {
    let http = MockHttp::new(
        StatusCode::OK,
        br#"{"access_token":"fresh","refresh_token":"rotated","expires_in":3600,"scope":"user:inference user:projects:read"}"#,
    );
    let secret = json!({
        "access_token":"stale",
        "refresh_token":"old-refresh",
        "expires_at_ms":1,
        "account_uuid":"account",
        "scopes":["user:inference", "user:projects:read"]
    });
    let future = ClaudeCodeChannel.refresh(&secret, &http).unwrap();
    let refreshed = ready(future).unwrap();
    assert_eq!(refreshed["access_token"], "fresh");
    assert_eq!(refreshed["refresh_token"], "rotated");
    assert_eq!(refreshed["account_uuid"], "account");
    assert!(refreshed["expires_at_ms"].as_i64().unwrap() > 1);
    assert!(
        refreshed["device_id"]
            .as_str()
            .is_some_and(|id| id.len() == 64)
    );
    assert!(ClaudeCodeChannel.refresh_due(&secret).is_some());

    let captured = http.captured.lock().unwrap();
    let captured = captured.as_ref().unwrap();
    assert_eq!(captured.uri, "https://api.anthropic.com/v1/oauth/token");
    assert!(captured.profile);
    assert_eq!(captured.headers["anthropic-beta"], "oauth-2025-04-20");
    let body = std::str::from_utf8(&captured.body).unwrap();
    assert!(body.contains("grant_type=refresh_token"));
    assert!(body.contains("refresh_token=old-refresh"));
    assert!(body.contains("user%3Aprojects%3Aread"));
}

struct Captured {
    uri: String,
    headers: HeaderMap,
    body: Bytes,
    profile: bool,
}

struct MockHttp {
    captured: Mutex<Option<Captured>>,
    status: StatusCode,
    response: Bytes,
}

impl MockHttp {
    fn new(status: StatusCode, response: &'static [u8]) -> Self {
        Self {
            captured: Mutex::new(None),
            status,
            response: Bytes::from_static(response),
        }
    }
}

impl SimpleHttp for MockHttp {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, gproxy_channel_api::ChannelError>> {
        let profile =
            request.extensions().get::<TransportProfile>() == Some(&TransportProfile::ClaudeCode);
        let (parts, body) = request.into_parts();
        *self.captured.lock().unwrap() = Some(Captured {
            uri: parts.uri.to_string(),
            headers: parts.headers,
            body,
            profile,
        });
        let status = self.status;
        let response = self.response.clone();
        Box::pin(async move {
            http::Response::builder()
                .status(status)
                .body(response)
                .map_err(|error| gproxy_channel_api::ChannelError::Refresh(error.to_string()))
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
