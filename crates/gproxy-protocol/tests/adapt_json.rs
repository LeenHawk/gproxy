use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::{JsonInvocation, invoke_json},
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, Method, StatusCode},
    transform::TransformErrorKind,
    wire::openai::count_tokens::{CountTokensRequestBody, CountTokensResponseBody},
};
use serde_json::json;
use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll},
    time::Duration,
};

struct Host {
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
    response: Mutex<Option<WireResponse<HttpBody>>>,
    read_limit: u64,
    write_limit: u64,
}
impl Upstream for Host {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            self.sent.lock().unwrap().push(request);
            Ok(self.response.lock().unwrap().take().expect("no retries"))
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected WS call")
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(30),
            stream_idle: Duration::from_secs(5),
            read_bytes: self.read_limit,
            write_bytes: self.write_limit,
            ws_frame_bytes: 1024,
        }
    }
}
fn host(status: StatusCode, body: HttpBody) -> Host {
    Host {
        sent: Mutex::default(),
        response: Mutex::new(Some(WireResponse {
            status,
            headers: HeaderMap::new(),
            body,
        })),
        read_limit: 1024,
        write_limit: 1024,
    }
}
fn request() -> WireRequest<CountTokensRequestBody> {
    let body =
        serde_json::from_value(json!({"model":"target","input":"hello","unknown":{"bad":true}}))
            .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("content-length", "1".parse().unwrap());
    headers.insert("content-encoding", "gzip".parse().unwrap());
    headers.insert("transfer-encoding", "chunked".parse().unwrap());
    WireRequest {
        method: Method::POST,
        path: "/v1/responses/input_tokens".into(),
        query: None,
        headers,
        body,
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 1024,
        max_value_bytes: 1024,
        max_body_bytes: 1024,
        max_line_bytes: 1024,
        max_part_bytes: 1024,
        max_parts: 8,
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture should be immediately ready"),
    }
}
fn invoke(
    host: &Host,
    request: WireRequest<CountTokensRequestBody>,
) -> Result<JsonInvocation<CountTokensResponseBody>, gproxy_protocol::transform::TransformError> {
    ready(invoke_json(host, &(), request, limits()))
}

#[test]
fn split_json_is_decoded_and_extensions_cannot_cross_the_invocation() {
    let body = HttpBody::Stream(Box::pin(futures_util::stream::iter([
        Ok(Bytes::from_static(
            b"{\"object\":\"response.input_tokens\",\"input_",
        )),
        Ok(Bytes::from_static(
            b"tokens\":12,\"unknown\":{\"bad\":true}}",
        )),
    ])));
    let host = host(StatusCode::OK, body);
    {
        let mut response = host.response.lock().unwrap();
        let headers = &mut response.as_mut().unwrap().headers;
        headers.insert("content-length", "999".parse().unwrap());
        headers.insert("transfer-encoding", "chunked".parse().unwrap());
        headers.insert("x-request-id", "upstream-id".parse().unwrap());
    }
    let JsonInvocation::Success(result) = invoke(&host, request()).unwrap() else {
        panic!("success expected")
    };
    assert_eq!(result.body.input_tokens, 12);
    assert!(!result.headers.contains_key("content-length"));
    assert!(!result.headers.contains_key("transfer-encoding"));
    assert_eq!(result.headers["content-type"], "application/json");
    assert_eq!(result.headers["x-request-id"], "upstream-id");
    assert!(result.body.rest.is_empty());
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].headers["content-type"], "application/json");
    assert!(!sent[0].headers.contains_key("content-length"));
    assert!(!sent[0].headers.contains_key("content-encoding"));
    assert!(!sent[0].headers.contains_key("transfer-encoding"));
    let HttpBody::Bytes(bytes) = &sent[0].body else {
        panic!("encoded bytes expected")
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(bytes).unwrap(),
        json!({"model":"target","input":"hello"})
    );
}

#[test]
fn upstream_http_failure_is_returned_without_body_consumption_or_retry() {
    let body = HttpBody::Stream(Box::pin(futures_util::stream::poll_fn(
        |_| -> Poll<Option<Result<Bytes, gproxy_protocol::connection::TransportError>>> {
            panic!("rejected body was polled")
        },
    )));
    let host = host(StatusCode::TOO_MANY_REQUESTS, body);
    let JsonInvocation::Rejected(response) = invoke(&host, request()).unwrap() else {
        panic!("rejection expected")
    };
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn host_byte_limits_are_obeyed_even_when_codec_limits_are_larger() {
    let mut host = host(StatusCode::OK, HttpBody::Bytes(Bytes::from_static(b"{}")));
    host.write_limit = 1;
    assert_eq!(
        invoke(&host, request()).unwrap_err().kind(),
        TransformErrorKind::Limit
    );
    assert!(host.sent.lock().unwrap().is_empty());
    host.write_limit = 1024;
    host.read_limit = 1;
    assert_eq!(
        invoke(&host, request()).unwrap_err().kind(),
        TransformErrorKind::Limit
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn invalid_target_path_fails_before_send_and_invalid_success_json_is_not_retried() {
    let host = host(
        StatusCode::OK,
        HttpBody::Bytes(Bytes::from_static(b"{\"input_tokens\":")),
    );
    for path in [
        "https://example.com/a",
        "//example.com/a",
        "/a?key=x",
        "/a#fragment",
        "/\\evil",
    ] {
        let mut request = request();
        request.path = path.into();
        assert_eq!(
            invoke(&host, request).unwrap_err().kind(),
            TransformErrorKind::InvalidInput
        );
    }
    assert!(host.sent.lock().unwrap().is_empty());
    assert_eq!(
        invoke(&host, request()).unwrap_err().kind(),
        TransformErrorKind::InvalidResult
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
