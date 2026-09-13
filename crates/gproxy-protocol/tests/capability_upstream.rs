use std::{
    future::Future,
    task::{Context, Poll},
};

use bytes::Bytes;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, Upstream, UpstreamConnection,
    },
    connection::{HeaderMap, Method, StatusCode},
};

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synchronous contract fake unexpectedly pending"),
    }
}

fn limits() -> CapabilityLimits {
    CapabilityLimits {
        operation_total: std::time::Duration::from_secs(5),
        stream_idle: std::time::Duration::from_secs(1),
        read_bytes: 1024,
        write_bytes: 1024,
        ws_frame_bytes: 1024,
    }
}

struct HandshakeFake;

impl Upstream for HandshakeFake {
    type Target = String;

    fn send<'a>(
        &'a self,
        _: &'a Self::Target,
        _: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async {
            Ok(WireResponse {
                status: StatusCode::TOO_MANY_REQUESTS,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from_static(b"http error body")),
            })
        })
    }

    fn connect<'a>(
        &'a self,
        _: &'a Self::Target,
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async {
            Ok(UpstreamConnection::Rejected(WireResponse {
                status: StatusCode::BAD_REQUEST,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from_static(b"handshake error body")),
            }))
        })
    }

    fn limits(&self) -> CapabilityLimits {
        limits()
    }
}

fn request<B>(body: B) -> WireRequest<B> {
    WireRequest {
        method: Method::GET,
        path: "/socket".into(),
        query: None,
        headers: HeaderMap::new(),
        body,
    }
}

#[test]
fn non_success_http_and_rejected_handshake_are_responses_with_bodies() {
    let fake = HandshakeFake;
    let target = "origin-a".to_owned();
    let response = ready(fake.send(&target, request(HttpBody::Bytes(Bytes::new())))).unwrap();
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
    let HttpBody::Bytes(body) = response.body else {
        panic!("expected buffered body")
    };
    assert_eq!(body, Bytes::from_static(b"http error body"));

    let connection = ready(fake.connect(&target, request(()))).unwrap();
    let UpstreamConnection::Rejected(response) = connection else {
        panic!("expected rejected handshake")
    };
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let HttpBody::Bytes(body) = response.body else {
        panic!("expected buffered body")
    };
    assert_eq!(body, Bytes::from_static(b"handshake error body"));
}

#[test]
fn unsupported_transport_errors_remain_structured() {
    let error = CapabilityError::new(
        CapabilityErrorKind::Unsupported,
        CapabilityErrorStage::Start,
        "not available",
    );
    assert_eq!(error.kind(), CapabilityErrorKind::Unsupported);
    assert_eq!(error.stage(), CapabilityErrorStage::Start);
}
