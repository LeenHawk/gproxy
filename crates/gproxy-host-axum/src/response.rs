//! Turning an execution into an HTTP response, without buffering it and
//! without dropping the request's lease early.
//!
//! # Why a wrapper stream
//!
//! Four things have to outlive `App::call`:
//!
//! - the [`Admitted`], because a concurrency permit measures requests **in
//!   flight** and a stream that is still running is one;
//! - the [`DownstreamCapture`], because the response it records has not been
//!   written yet;
//! - core's `UsageCompletion`, because awaiting it is what settles the request
//!   and writes its usage row. Dropping it unread loses the metering.
//! - the request's [`CancelOnDrop`], because the client can still leave.
//!
//! All four are moved **into** the response body. [`LeasedBody`] owns them,
//! feeds each chunk to the capture as it passes, and on the last chunk awaits
//! the settlement, writes the capture and releases the lease. There is nowhere
//! else the values live, so no call site can forget one — which is the whole
//! reason it is a stream that owns them rather than a guard somebody holds.
//!
//! # A client that leaves cancels the upstream call
//!
//! A client that hangs up mid-stream drops the body. Hyper says nothing about
//! it — the drop *is* the notification — so the token that stops the upstream
//! call lives in the value being dropped. [`LeasedBody`] holds the whole
//! [`Trailer`], so the same drop that returns the lease also cancels the call
//! nobody is going to read, and core settles it as `UsageState::Cancelled`
//! instead of paying for the rest of the answer.
//!
//! The one thing that must **not** cancel is a response that ended by itself.
//! Core reads the token when it decides between `Completed` and `Cancelled`, so
//! a token fired after the last byte would write a lie into the usage row. See
//! [`CancelOnDrop`] for where the disarm happens and why that point is the
//! right one.

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
use tokio_util::sync::CancellationToken;

/// One request's cancellation token, which fires when this value is dropped
/// unless it has been disarmed first.
///
/// A client leaving is a **drop**, in both of the two places it can happen: the
/// handler future is dropped when the connection dies before a head was
/// written, and the response body is dropped when it dies mid-stream. Neither
/// is an event anything calls the host back about, so the token is owned by the
/// value whose destructor *is* the event, and every path that ends a request
/// normally has exactly one thing to do: say so.
///
/// # Why the disarm matters more than the cancel
///
/// Core reads the token at settlement time to tell `UsageState::Completed` from
/// `UsageState::Cancelled`, and its response stream selects against the token on
/// every chunk. A token fired after the last byte would therefore turn a
/// finished request into a cancelled one in the usage row, and could interrupt
/// the very settlement the [`Trailer`] is about to await. So the guard is
/// disarmed the moment the upstream stream is over — clean end or transport
/// failure, both are *its* end rather than the client's — which is
/// [`Trailer::finish`], the one funnel every normal ending goes through.
pub struct CancelOnDrop {
    token: CancellationToken,
    armed: bool,
}

impl CancelOnDrop {
    /// A fresh token, armed. One per request: it is the unit core cancels.
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            armed: true,
        }
    }

    /// The copy that goes on
    /// [`DataPlaneRequest::cancellation`](gproxy_app::DataPlaneRequest).
    /// Cloned rather than handed over, because the guard keeps the only copy
    /// that can fire.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// This request ended on its own terms; dropping the guard must not claim
    /// otherwise.
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// The client is known to be gone *now*, rather than whenever this value
    /// happens to drop. The websocket pump needs it: it sees the departure as a
    /// stream ending and still has a closing handshake and a settlement to run
    /// afterwards, and core has to know before those.
    pub fn cancel(&mut self) {
        self.armed = false;
        self.token.cancel();
    }
}

impl Default for CancelOnDrop {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CancelOnDrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelOnDrop")
            .field("armed", &self.armed)
            .field("cancelled", &self.token.is_cancelled())
            .finish()
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.token.cancel();
        }
    }
}

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
///
/// `cancel` is the guard the ingress handler made and put on the request. It
/// moves in here, so from the moment the head is written the body is what owns
/// the client's departure.
pub fn streamed<C>(app: Arc<App<C>>, outcome: CallOutcome, cancel: CancelOnDrop) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let CallOutcome {
        execution,
        admitted,
        capture,
    } = outcome;
    let (response, usage) = execution.into_parts();
    leased(
        Trailer::new(app, admitted, capture, usage, cancel),
        response,
    )
}

/// The same, for a caller that already holds the decision.
///
/// [`crate::websocket`] uses it for the one answer a handshake can give over
/// HTTP: an upstream that refused the upgrade, whose whole response is relayed
/// while the lease it was admitted under is released on the last byte.
pub(crate) fn leased<C>(trailer: Trailer<C>, response: WireResponse<HttpBody>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let status = response.status;
    let headers = sanitize(response.headers);
    let body = LeasedBody {
        inner: Some(chunks(response.body)),
        trailer: Some(trailer),
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
///
/// `sec-websocket-accept` and `sec-websocket-key` go with them: they are the
/// proof of *one* handshake, computed from the key that side sent, and the
/// gateway's 101 to the client carries its own.
pub(crate) fn sanitize(mut headers: HeaderMap) -> HeaderMap {
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
        "sec-websocket-accept",
        "sec-websocket-key",
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

/// What a response body — or a socket — owns until it ends.
///
/// Named for the HTTP case it was written for, and reused verbatim by
/// [`crate::websocket`]: a socket has exactly the same four things to keep
/// alive and exactly the same order to release them in, and two copies of that
/// order would be two chances to get it wrong.
pub(crate) struct Trailer<C> {
    app: Arc<App<C>>,
    admitted: Admitted,
    capture: Option<DownstreamCapture>,
    usage: UsageCompletion,
    /// Armed for as long as this value lives, so dropping it — which is what a
    /// client hanging up does — cancels the upstream call.
    cancel: CancelOnDrop,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Trailer<C> {
    pub(crate) fn new(
        app: Arc<App<C>>,
        admitted: Admitted,
        capture: Option<DownstreamCapture>,
        usage: UsageCompletion,
        cancel: CancelOnDrop,
    ) -> Self {
        Self {
            app,
            admitted,
            capture,
            usage,
            cancel,
        }
    }

    /// The capture to feed, while the response or the socket is still running.
    pub(crate) fn capture_mut(&mut self) -> Option<&mut DownstreamCapture> {
        self.capture.as_mut()
    }

    /// The client is gone and the holder knows it before it lets go of this
    /// value. Only the websocket pump needs it; an HTTP body learns the same
    /// thing by being dropped.
    pub(crate) fn cancel(&mut self) {
        self.cancel.cancel();
    }

    /// Settle the request and give the charges back. Awaiting the
    /// `UsageCompletion` is not optional: it is what makes core write the
    /// usage row, and [`DownstreamCapture::settle`] awaits it for us.
    fn finish(self, outcome: CaptureOutcome) -> TailFuture {
        let Self {
            app,
            mut admitted,
            capture,
            usage,
            mut cancel,
        } = self;
        // **The one disarm.** Every normal ending — a body that ran out, an
        // upstream that failed mid-body, a refused handshake, a closed socket —
        // arrives here, and all of them mean the upstream is already done. The
        // token must not fire afterwards: core reads it to choose between
        // `Completed` and `Cancelled`, so a late cancel would record a finished
        // request as abandoned, and the settlement awaited just below is what it
        // would be racing.
        cancel.disarm();
        drop(cancel);
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

    /// The same, for a caller that can simply await it — a socket pump runs in
    /// its own task and has no stream to thread a future through.
    pub(crate) async fn settle(self, outcome: CaptureOutcome) {
        self.finish(outcome).await;
    }
}

/// The response body of a data-plane call: the upstream's chunks, plus
/// everything that has to stay alive while they are being written.
///
/// While `inner` is `Some` the stream has not reached its end, so the
/// [`Trailer`] it still holds is armed: hyper dropping this value is a client
/// that hung up mid-stream, and the drop cancels the upstream call. Once
/// `inner` is `None` the `Trailer` has been handed to [`Trailer::finish`],
/// which disarmed it — so a body that is dropped while its settlement is still
/// running cancels nothing.
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
