//! Turning an execution into an HTTP response, without buffering it and
//! without dropping the request's lease early.
//!
//! # Why a wrapper stream
//!
//! Three things have to outlive `App::call`:
//!
//! - the [`Admitted`], because a concurrency permit measures requests **in
//!   flight** and a stream that is still running is one;
//! - the [`DownstreamCapture`], because the response it records has not been
//!   written yet;
//! - core's `UsageCompletion`, because awaiting it is what settles the request
//!   and writes its usage row. Dropping it unread loses the metering.
//!
//! All three are moved **into** the response body. [`LeasedBody`] owns them,
//! feeds each chunk to the capture as it passes, and on the last chunk awaits
//! the settlement, writes the capture and releases the lease. There is nowhere
//! else the values live, so no call site can forget one — which is the whole
//! reason it is a stream that owns them rather than a guard somebody holds.
//!
//! A client that hangs up mid-stream drops the body instead. The lease is then
//! returned by `Admitted`'s own drop path, the capture is lost and the
//! settlement does not run; that is the documented cost of an abandoned
//! response, and it is the same cost core's observer pays.

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};

use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use futures_core::Stream;
use gproxy_app::{
    Admitted, App, CallOutcome,
    capture::{CaptureOutcome, DownstreamCapture},
};
use gproxy_core::UsageCompletion;
use gproxy_protocol::{
    HttpBody, WireResponse,
    connection::{Bytes, TransportError},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderName, StatusCode};

/// An upstream response forwarded as it is, with nothing held open.
///
/// For the surfaces that have no lease to keep: a publication download, a
/// vendor service call. The body still streams — a service may answer with a
/// large file — it simply has nothing to release at the end.
pub fn passthrough(status: StatusCode, headers: HeaderMap, body: HttpBody) -> Response {
    build(status, sanitize(headers), into_body(body))
}

/// The same for a whole [`WireResponse`], which is what a channel's service
/// call answers with.
pub fn passthrough_wire(response: WireResponse<HttpBody>) -> Response {
    passthrough(response.status, response.headers, response.body)
}

/// A data-plane response: streamed straight through, with the request's
/// decision held open until the last byte.
pub fn streamed<C>(app: Arc<App<C>>, outcome: CallOutcome) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let CallOutcome {
        execution,
        admitted,
        capture,
    } = outcome;
    let (response, usage) = execution.into_parts();
    let status = response.status;
    let headers = sanitize(response.headers);
    let body = LeasedBody {
        inner: Some(chunks(response.body)),
        trailer: Some(Trailer {
            app,
            admitted,
            capture,
            usage,
        }),
        tail: None,
        failure: None,
    };
    build(status, headers, Body::from_stream(body))
}

fn build(status: StatusCode, headers: HeaderMap, body: Body) -> Response {
    let mut response = body.into_response();
    *response.status_mut() = status;
    // `into_response` sets a content-type of its own; the upstream's headers
    // are the authority for what this body is.
    *response.headers_mut() = headers;
    response
}

/// Drop the hop-by-hop headers of the upstream's own connection.
///
/// RFC 9110 §7.6.1: `Connection` and everything it nominates describe the
/// link the gateway terminated, not the response. Forwarding
/// `transfer-encoding: chunked` in particular makes hyper frame a body it has
/// already framed, and forwarding a nominated header leaks an upstream's
/// connection state to the client.
fn sanitize(mut headers: HeaderMap) -> HeaderMap {
    let nominated: Vec<HeaderName> = headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect();
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
    headers
}

fn into_body(body: HttpBody) -> Body {
    match body {
        HttpBody::Bytes(bytes) => Body::from(bytes),
        HttpBody::Stream(stream) => Body::from_stream(stream),
    }
}

/// A body as a stream either way, so the buffered and the streamed case share
/// one settlement path. A buffered body still has to reach the capture and
/// still has to await its settlement.
fn chunks(body: HttpBody) -> ChunkStream {
    match body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
        HttpBody::Stream(stream) => stream,
    }
}

type ChunkStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + Send + 'static>>;
type TailFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// What the response body owns until it ends.
struct Trailer<C> {
    app: Arc<App<C>>,
    admitted: Admitted,
    capture: Option<DownstreamCapture>,
    usage: UsageCompletion,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Trailer<C> {
    /// Settle the request and give the charges back. Awaiting the
    /// `UsageCompletion` is not optional: it is what makes core write the
    /// usage row, and [`DownstreamCapture::settle`] awaits it for us.
    fn finish(self, outcome: CaptureOutcome) -> TailFuture {
        let Self {
            app,
            mut admitted,
            capture,
            usage,
        } = self;
        Box::pin(async move {
            match capture {
                Some(capture) => {
                    // The request has already been answered; a logging failure
                    // must not become one.
                    let _ = capture.settle(app.gproxy().store(), outcome, usage).await;
                }
                None => {
                    if let Err(error) = usage.await {
                        tracing::warn!(%error, "settlement failed after the response was written");
                    }
                }
            }
            // Awaited rather than left to the drop path, which has to spawn.
            admitted.release().await;
        })
    }
}

/// The response body of a data-plane call: the upstream's chunks, plus
/// everything that has to stay alive while they are being written.
pub struct LeasedBody<C> {
    inner: Option<ChunkStream>,
    trailer: Option<Trailer<C>>,
    tail: Option<TailFuture>,
    /// An upstream failure, held back until the settlement has run so the
    /// request is still recorded. Delivered as the stream's last item.
    failure: Option<TransportError>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Stream for LeasedBody<C> {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // `LeasedBody` holds only `Unpin`-safe owned values behind pointers, so
        // it is moved out of the pin once rather than projected field by field.
        let this = self.get_mut();
        loop {
            if let Some(tail) = this.tail.as_mut() {
                ready!(tail.as_mut().poll(context));
                this.tail = None;
                // `Some(Err(..))` when the upstream failed mid-body, `None`
                // when it ended cleanly. Either way the settlement has run.
                return Poll::Ready(this.failure.take().map(Err));
            }
            let Some(inner) = this.inner.as_mut() else {
                return Poll::Ready(None);
            };
            match ready!(inner.as_mut().poll_next(context)) {
                Some(Ok(bytes)) => {
                    if let Some(capture) = this
                        .trailer
                        .as_mut()
                        .and_then(|trailer| trailer.capture.as_mut())
                    {
                        capture.record_response_chunk(&bytes);
                    }
                    return Poll::Ready(Some(Ok(bytes)));
                }
                Some(Err(error)) => {
                    this.inner = None;
                    this.failure = Some(error);
                    this.tail = this
                        .trailer
                        .take()
                        .map(|trailer| trailer.finish(CaptureOutcome::Interrupted));
                }
                None => {
                    this.inner = None;
                    this.tail = this
                        .trailer
                        .take()
                        .map(|trailer| trailer.finish(CaptureOutcome::Complete));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    #[test]
    fn the_upstreams_connection_headers_do_not_reach_the_client() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONNECTION,
            HeaderValue::from_static("keep-alive, x-upstream-token"),
        );
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        headers.insert("x-upstream-token", HeaderValue::from_static("secret"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let headers = sanitize(headers);
        assert!(!headers.contains_key(http::header::CONNECTION));
        assert!(!headers.contains_key("transfer-encoding"));
        assert!(
            !headers.contains_key("x-upstream-token"),
            "a header the upstream nominated in `Connection` is its own, not the client's"
        );
        assert_eq!(headers["content-type"], "application/json");
    }
}
