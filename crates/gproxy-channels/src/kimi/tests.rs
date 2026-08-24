use std::sync::Mutex;

use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, Channel, PrepareCtx, SimpleHttp, UsageCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::KimiChannel;

fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn catalog_and_credential_modes_select_only_declared_targets() {
    let descriptor = KimiChannel.descriptor();
    assert_eq!(descriptor.supports.len(), 17);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let api = KimiChannel
        .select_support(claude, &json!({"api_key":"key"}))
        .unwrap();
    assert_eq!(
        api.target,
        content(Operation::GenerateContent, Kind::OpenAiChat)
    );
    let oauth = KimiChannel
        .select_support(claude, &json!({"auth_kind":"oauth","access_token":"token"}))
        .unwrap();
    assert_eq!(oauth.target, claude);
    assert!(
        KimiChannel
            .select_support(
                OperationKey::family(Operation::CreateEmbedding, WireFamily::OpenAi),
                &json!({"api_key":"key"}),
            )
            .is_none()
    );
    assert_eq!(crate::canonical_channel_id("kimiapi"), "kimi");
    assert_eq!(crate::canonical_channel_id("kimicode"), "kimi");
}

#[test]
fn prepares_api_and_oauth_urls_auth_identity_and_models() {
    let body = Bytes::from_static(br#"{"model":"route","messages":[]}"#);
    let api_secret = json!({"api_key":"moonshot-key"});
    let api = KimiChannel
        .prepare(PrepareCtx {
            key: content(Operation::GenerateContent, Kind::OpenAiChat),
            stream: false,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "moonshot-v1-128k",
            provider_settings: &json!({}),
            secret: &api_secret,
        })
        .unwrap();
    assert_eq!(
        api.request.uri(),
        "https://api.moonshot.cn/v1/chat/completions"
    );
    assert_eq!(
        api.request.headers()["authorization"],
        "Bearer moonshot-key"
    );
    let api_body: Value = serde_json::from_slice(api.request.body()).unwrap();
    assert_eq!(api_body["model"], "moonshot-v1-128k");

    let oauth_secret = json!({
        "auth_kind":"oauth","access_token":"access","refresh_token":"refresh",
        "device_id":"device-1"
    });
    let claude_body = Bytes::from_static(br#"{"model":"route","messages":[],"max_tokens":8}"#);
    let settings = json!({
        "base_url":"https://unused.example",
        "endpoints":{"claude_messages":"https://coding.example/{model}?fixed=1"}
    });
    let oauth = KimiChannel
        .prepare(PrepareCtx {
            key: content(Operation::GenerateContent, Kind::ClaudeMessages),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &HeaderMap::new(),
            body: &claude_body,
            upstream_model: "kimi-for-coding",
            provider_settings: &settings,
            secret: &oauth_secret,
        })
        .unwrap();
    assert_eq!(
        oauth.request.uri(),
        "https://coding.example/kimi-for-coding?fixed=1"
    );
    assert_eq!(oauth.request.headers()["x-api-key"], "access");
    assert_eq!(oauth.request.headers()["x-msh-device-id"], "device-1");
    assert!(oauth.request.headers().get("authorization").is_none());
}

#[test]
fn refreshes_rotating_oauth_and_preserves_kimi_cache_usage() {
    let http = MockHttp::new(json!({
        "access_token":"new-access","refresh_token":"new-refresh","expires_in":3600
    }));
    let secret = json!({
        "auth_kind":"oauth","access_token":"old","refresh_token":"old-refresh",
        "expires_at_ms":1,"device_id":"device-1","future":"kept"
    });
    let settings = json!({"oauth_host":"https://oauth.example"});
    let mut future = KimiChannel.refresh(&secret, &settings, &http).unwrap();
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    let std::task::Poll::Ready(refreshed) = future.as_mut().poll(&mut context) else {
        panic!("mock refresh future must be ready")
    };
    let refreshed = refreshed.unwrap();
    assert_eq!(refreshed["access_token"], "new-access");
    assert_eq!(refreshed["refresh_token"], "new-refresh");
    assert_eq!(refreshed["future"], "kept");
    assert_eq!(
        http.uri.lock().unwrap().as_deref(),
        Some("https://oauth.example/api/oauth/token")
    );

    let request = Bytes::from_static(br#"{"model":"moonshot","messages":[]}"#);
    let response = Bytes::from_static(
        br#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":4,"total_tokens":14,"cached_tokens":3}}"#,
    );
    let headers = HeaderMap::new();
    let usage = KimiChannel
        .extract_usage(UsageCtx {
            key: content(Operation::GenerateContent, Kind::OpenAiChat),
            request_body: &request,
            response_headers: &headers,
            response_body: &response,
        })
        .unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cached_input_tokens
        ),
        (10, 4, 3)
    );
}

struct MockHttp {
    response: Bytes,
    uri: Mutex<Option<String>>,
}

impl MockHttp {
    fn new(response: Value) -> Self {
        Self {
            response: Bytes::from(response.to_string()),
            uri: Mutex::new(None),
        }
    }
}

impl SimpleHttp for MockHttp {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, gproxy_channel_api::ChannelError>> {
        *self.uri.lock().unwrap() = Some(request.uri().to_string());
        let response = self.response.clone();
        Box::pin(async move { Ok(http::Response::new(response)) })
    }
}
