use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, SurfaceRequest};
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
