use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use tokio::sync::OwnedSemaphorePermit;

use gproxy_core::{CoreError, ExecOutcome, ResponseBody};

pub(crate) struct HostResponse(
    Result<ExecOutcome, CoreError>,
    Option<WebSocketUpgrade>,
    OwnedSemaphorePermit,
    String,
);

impl HostResponse {
    pub(crate) fn new(
        result: Result<ExecOutcome, CoreError>,
        upgrade: Option<WebSocketUpgrade>,
        permit: OwnedSemaphorePermit,
        request_id: String,
    ) -> Self {
        Self(result, upgrade, permit, request_id)
    }
}

impl IntoResponse for HostResponse {
    fn into_response(self) -> Response {
        let Self(result, upgrade, permit, request_id) = self;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => return core_error(error, permit, &request_id),
        };
        if upgrade.is_some()
            && outcome.status.is_success()
            && !matches!(&outcome.body, ResponseBody::WebSocket(_))
        {
            return local_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "websocket request returned a non-websocket response",
                permit,
                &request_id,
            );
        }
        let ExecOutcome {
            status,
            headers,
            body,
            ..
        } = outcome;
        let headers = sanitize(headers, &request_id);
        match body {
            ResponseBody::Full(bytes) => response(status, headers, full_body(bytes, permit)),
            ResponseBody::Stream(stream) => response(
                status,
                headers,
                Body::from_stream(PermitStream::new(stream, permit)),
            ),
            ResponseBody::WebSocket(upstream) => {
                let Some(upgrade) = upgrade else {
                    return local_error(
                        StatusCode::BAD_REQUEST,
                        "websocket upgrade required",
                        permit,
                        &request_id,
                    );
                };
                let mut response = upgrade
                    .on_upgrade(move |socket| crate::websocket::pump(socket, upstream, permit));
                append_missing(response.headers_mut(), &headers);
                response
            }
        }
    }
}

pub(crate) fn buffered_response(
    buffered: http::Response<Bytes>,
    permit: OwnedSemaphorePermit,
    request_id: &str,
) -> Response {
    let (parts, body) = buffered.into_parts();
    response(
        parts.status,
        sanitize(parts.headers, request_id),
        full_body(body, permit),
    )
}

fn response(status: StatusCode, headers: HeaderMap, body: Body) -> Response {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn core_error(error: CoreError, permit: OwnedSemaphorePermit, request_id: &str) -> Response {
    let status = error.status();
    json_response(
        status,
        Bytes::from(error.body_json().to_string()),
        permit,
        request_id,
    )
}

fn local_error(
    status: StatusCode,
    message: &'static str,
    permit: OwnedSemaphorePermit,
    request_id: &str,
) -> Response {
    let body = format!(r#"{{"error":{{"message":"{message}"}}}}"#);
    json_response(status, Bytes::from(body), permit, request_id)
}

fn json_response(
    status: StatusCode,
    body: Bytes,
    permit: OwnedSemaphorePermit,
    request_id: &str,
) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert("x-request-id", request_id_value(request_id));
    response(status, headers, full_body(body, permit))
}

fn full_body(bytes: Bytes, permit: OwnedSemaphorePermit) -> Body {
    let stream = Box::pin(futures_util::stream::once(async move {
        Ok::<Bytes, Infallible>(bytes)
    }));
    Body::from_stream(PermitStream::new(stream, permit))
}

fn sanitize(mut headers: HeaderMap, request_id: &str) -> HeaderMap {
    let nominated = headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect::<Vec<_>>();
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
    for name in nominated {
        headers.remove(name);
    }
    headers.insert("x-request-id", request_id_value(request_id));
    headers
}

fn append_missing(target: &mut HeaderMap, source: &HeaderMap) {
    for name in source.keys() {
        if target.contains_key(name) {
            continue;
        }
        for value in source.get_all(name) {
            target.append(name, value.clone());
        }
    }
}

fn request_id_value(request_id: &str) -> HeaderValue {
    HeaderValue::from_str(request_id).expect("host-generated request id is a valid header value")
}

struct PermitStream<S> {
    inner: S,
    permit: Option<OwnedSemaphorePermit>,
}

impl<S> PermitStream<S> {
    fn new(inner: S, permit: OwnedSemaphorePermit) -> Self {
        Self {
            inner,
            permit: Some(permit),
        }
    }
}

impl<S: Stream + Unpin> Stream for PermitStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = Pin::new(&mut self.inner).poll_next(context);
        if matches!(&result, Poll::Ready(None)) {
            self.permit.take();
        }
        result
    }
}
