use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, SessionPrepareCtx, SurfaceRequest};
use gproxy_protocol::{Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::json;

use super::super::CodexChannel;

#[test]
fn raw_sdp_realtime_call_preserves_body_and_content_type() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        "application/sdp; charset=utf-8".parse().unwrap(),
    );
    let body = Bytes::from_static(b"v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\n");
    let prepared = CodexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::CreateRealtimeCall, WireFamily::OpenAi),
            stream: false,
            method: &Method::POST,
            path: "/v1/realtime/calls",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "gpt-realtime",
            provider_settings: &json!({}),
            secret: &json!({"access_token":"token"}),
        })
        .expect("raw SDP request");
    assert_eq!(prepared.request.body(), &body);
    assert_eq!(
        prepared.request.headers()[http::header::CONTENT_TYPE],
        "application/sdp; charset=utf-8"
    );
}

#[test]
fn sideband_reuses_selected_codex_credential_and_fixed_openai_uri() {
    let request_body = Bytes::from_static(
        br#"{"sdp":"v=offer","session":{"type":"realtime","model":"gpt-realtime"}}"#,
    );
    let response_headers = HeaderMap::from_iter([(
        http::header::LOCATION,
        "/v1/realtime/calls/rtc_selected".parse().unwrap(),
    )]);
    let mut request_headers = HeaderMap::new();
    request_headers.insert("session-id", "setup-session".parse().unwrap());
    request_headers.insert("x-client-request-id", "setup-request".parse().unwrap());
    let prepared = CodexChannel.session_preparer().expect("session preparer")(SessionPrepareCtx {
        request_body: &request_body,
        request_headers: &request_headers,
        response_headers: &response_headers,
        upstream_model: "gpt-realtime",
        secret: &json!({"access_token":"oauth-token","account_id":"acct-1"}),
    })
    .unwrap();
    assert_eq!(
        prepared.request.request.uri(),
        "wss://api.openai.com/v1/realtime?call_id=rtc_selected"
    );
    assert_eq!(
        prepared.request.request.headers()[http::header::AUTHORIZATION],
        "Bearer oauth-token"
    );
    assert_eq!(
        prepared.request.request.headers()["chatgpt-account-id"],
        "acct-1"
    );
    assert_eq!(
        prepared.request.request.headers()["originator"],
        "codex_cli_rs"
    );
    assert!(
        prepared.request.request.headers()[http::header::USER_AGENT]
            .to_str()
            .unwrap()
            .starts_with("codex_cli_rs/")
    );
    assert_eq!(
        prepared.request.request.headers()["session-id"],
        "setup-session"
    );
    assert_eq!(
        prepared.request.request.headers()["x-client-request-id"],
        "setup-request"
    );
    assert!(prepared.request.websocket);
    assert!(prepared.request.profile.is_some());
    assert_eq!(
        prepared.termination.request.uri(),
        "https://api.openai.com/v1/realtime/calls/rtc_selected/hangup"
    );
    assert_eq!(prepared.termination.request.method(), http::Method::POST);
    assert_eq!(
        prepared.termination.request.headers()[http::header::AUTHORIZATION],
        "Bearer oauth-token"
    );
    assert!(!prepared.termination.websocket);
    assert!(prepared.termination.profile.is_some());
}

#[test]
fn pairing_surface_preserves_remote_control_bearer() {
    let prepared = super::super::prepare::surface(
        &SurfaceRequest {
            label: "remote_control_token",
            key: None,
            stream: false,
            method: Method::POST,
            upstream_path: "/wham/remote/control/server/pair".into(),
            query: None,
            headers: HeaderMap::from_iter([(
                http::header::AUTHORIZATION,
                "Bearer remote-token".parse().unwrap(),
            )]),
            body: Bytes::from_static(b"{}"),
            credential: None,
        },
        false,
        &json!({}),
        &json!({"access_token":"oauth-token"}),
    )
    .expect("pairing request");
    assert_eq!(
        prepared.request.headers()[http::header::AUTHORIZATION],
        "Bearer remote-token"
    );
}
