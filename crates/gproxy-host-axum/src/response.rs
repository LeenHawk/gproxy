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
//! feeds each chunk to the capture as it passes, and at the end hands them to
//! a settlement that writes the usage and the capture and releases the lease,
//! without making the client's last byte wait for it. There is nowhere else
//! the values live, so no call site can forget one — which is the whole reason
//! it is a stream that owns them rather than a guard somebody holds.
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
    let declared = headers
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let body = LeasedBody::new(chunks(response.body), trailer, declared);
    build(status, headers, Body::from_stream(crate::send(body)))
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
///
/// `set-cookie` too, though it is end-to-end: the upstream's cookies belong to
/// the gateway's session with it — for a channel signed in to a shared
/// account, that session *is* the account — and a client of the gateway is
/// not a party to it.
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
        "set-cookie",
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
        HttpBody::Stream(stream) => Body::from_stream(crate::send(stream)),
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

/// A chunk stream and a tail future, with the `Send` each target can honestly
/// promise. These mirror [`gproxy_protocol::connection::ByteStream`], which
/// makes the same split for the same reason: on wasm the bytes are being read
/// out of a JS `ReadableStream` that belongs to one isolate. [`crate::send`]
/// is what carries them across axum's bound.
#[cfg(not(target_arch = "wasm32"))]
type ChunkStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
type ChunkStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + 'static>>;
#[cfg(not(target_arch = "wasm32"))]
type TailFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
type TailFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// What a response body — or a socket — owns until it ends.
///
/// Named for the HTTP case it was written for, and reused verbatim by
/// [`crate::websocket`]: a socket has exactly the same four things to keep
/// alive and exactly the same order to release them in, and two copies of that
/// order would be two chances to get it wrong.
pub(crate) struct Trailer<C> {
    app: Arc<App<C>>,
    admitted: Option<Admitted>,
    capture: Option<DownstreamCapture>,
    usage: Option<UsageCompletion>,
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
            admitted: Some(admitted),
            capture,
            usage: Some(usage),
            cancel,
        }
    }

    pub(crate) fn service(
        app: Arc<App<C>>,
        capture: Option<DownstreamCapture>,
        cancel: CancelOnDrop,
    ) -> Self {
        Self {
            app,
            admitted: None,
            capture,
            usage: None,
            cancel,
        }
    }

    /// The capture to feed, while the response or the socket is still running.
    ///
    /// Only [`crate::websocket`]'s pump needs it — an HTTP body feeds its own
    /// capture from inside [`LeasedBody`] — so it is native-only for as long
    /// as sockets are.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn capture_mut(&mut self) -> Option<&mut DownstreamCapture> {
        self.capture.as_mut()
    }

    /// The client is gone and the holder knows it before it lets go of this
    /// value. Only the websocket pump needs it; an HTTP body learns the same
    /// thing by being dropped.
    #[cfg(not(target_arch = "wasm32"))]
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
            match (capture, usage) {
                (Some(capture), Some(usage)) => {
                    let _ = capture.settle(app.gproxy().store(), outcome, usage).await;
                }
                (Some(capture), None) => {
                    let _ = capture.finish(app.gproxy().store(), outcome).await;
                }
                (None, Some(usage)) => {
                    if let Err(error) = usage.await {
                        tracing::warn!(%error, "settlement failed after the response was written");
                    }
                }
                (None, None) => {}
            }
            if let Some(admitted) = admitted.as_mut() {
                admitted.release().await;
            }
        })
    }

    /// The same, for a caller that can simply await it — a socket pump runs in
    /// its own task and has no stream to thread a future through.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn settle(self, outcome: CaptureOutcome) {
        self.finish(outcome).await;
    }
}

/// The response body of a data-plane call: the upstream's chunks, plus
/// everything that has to stay alive while they are being written.
///
/// While `inner` is `Some` the stream has not reached its end, so the
/// [`Trailer`] it still holds is armed: hyper dropping this value is a client
/// that hung up mid-stream, and the drop cancels the upstream call.
///
/// # Where the end is
///
/// Not only where the upstream's stream ends. A buffered answer carries the
/// `content-length` the upstream declared, and hyper stops polling a body the
/// moment it has written that many bytes: the end-of-stream this type would
/// otherwise wait for is never asked for, and the body is dropped instead —
/// exactly what a client hanging up looks like. So the declared length is the
/// end too, noticed as the last chunk passes through.
///
/// # What happens at the end
///
/// The [`Trailer`] is disarmed and its settlement — core's usage, the capture,
/// the lease — runs on its own, so the client's last byte does not wait for the
/// database. What remains of the upstream stream goes with it and is drained
/// there, because core settles an exchange when that stream ends. Settlements
/// are bounded by [`SETTLING`]: past that, the last byte waits for a place, so
/// a database that falls behind slows its clients rather than piling up work.
pub struct LeasedBody<C> {
    inner: Option<ChunkStream>,
    trailer: Option<Trailer<C>>,
    /// `content-length`, when the upstream declared one; see above.
    declared: Option<u64>,
    written: u64,
    closing: Option<Closing>,
}

/// A body that has reached its end and is waiting to hand its settlement off.
struct Closing {
    /// What the client is owed once the hand-off is done: the last chunk, an
    /// upstream failure, or nothing at a plain end of stream.
    last: Option<Result<Bytes, TransportError>>,
    /// Native: a place among the settlements in flight. On wasm there is no
    /// task to hand to, so this is the settlement itself, awaited inline — a
    /// Worker's platform reads the body to its end, so it is always reached.
    wait: TailFuture,
    done: bool,
}

/// How many settlements may run behind responses that have already ended.
#[cfg(not(target_arch = "wasm32"))]
pub const SETTLING: usize = 2048;

#[cfg(not(target_arch = "wasm32"))]
fn settling() -> Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
    SEMAPHORE
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(SETTLING)))
        .clone()
}

/// Wait until every settlement already handed off has finished.
///
/// A response that has ended no longer holds its settlement, so shutting down
/// when the last connection closes would lose the ones still running. A host
/// awaits this after its connections have drained; a test awaits it before it
/// reads what a request wrote.
#[cfg(not(target_arch = "wasm32"))]
pub async fn settled() {
    let all = u32::try_from(SETTLING).unwrap_or(u32::MAX);
    if let Ok(permits) = settling().acquire_many_owned(all).await {
        drop(permits);
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> LeasedBody<C> {
    fn new(inner: ChunkStream, trailer: Trailer<C>, declared: Option<u64>) -> Self {
        Self {
            inner: Some(inner),
            trailer: Some(trailer),
            declared,
            written: 0,
            closing: None,
        }
    }

    /// The end: take the rest of the stream and the trailer, and arrange for
    /// them to settle, owing the client `last`.
    fn close(&mut self, outcome: CaptureOutcome, last: Option<Result<Bytes, TransportError>>) {
        let rest = self.inner.take();
        let trailer = self.trailer.take();
        let settle: TailFuture = Box::pin(async move {
            if let Some(mut rest) = rest {
                // Whatever the upstream still has after the declared length,
                // usually only its end-of-stream.
                while futures_util::StreamExt::next(&mut rest).await.is_some() {}
            }
            if let Some(trailer) = trailer {
                trailer.finish(outcome).await;
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        let wait: TailFuture = Box::pin(async move {
            let Ok(permit) = settling().acquire_owned().await else {
                // Never closed; if it were, settling inline is still correct.
                return settle.await;
            };
            tokio::spawn(async move {
                settle.await;
                drop(permit);
            });
        });
        #[cfg(target_arch = "wasm32")]
        let wait = settle;
        self.closing = Some(Closing {
            last,
            wait,
            done: false,
        });
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Stream for LeasedBody<C> {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // `LeasedBody` holds only `Unpin`-safe owned values behind pointers, so
        // it is moved out of the pin once rather than projected field by field.
        let this = self.get_mut();
        loop {
            if let Some(closing) = this.closing.as_mut() {
                if !closing.done {
                    ready!(closing.wait.as_mut().poll(context));
                    closing.done = true;
                }
                return Poll::Ready(closing.last.take());
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
                    this.written += bytes.len() as u64;
                    if this
                        .declared
                        .is_some_and(|declared| this.written >= declared)
                    {
                        this.close(CaptureOutcome::Complete, Some(Ok(bytes)));
                        continue;
                    }
                    return Poll::Ready(Some(Ok(bytes)));
                }
                // The failure is the client's last item, after the hand-off,
                // so the request is recorded however the body ended.
                Some(Err(error)) => {
                    this.inner = None;
                    this.close(CaptureOutcome::Interrupted, Some(Err(error)));
                }
                None => {
                    this.inner = None;
                    this.close(CaptureOutcome::Complete, None);
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
        headers.insert("set-cookie", HeaderValue::from_static("__session=upstream"));

        let headers = sanitize(headers);
        assert!(
            !headers.contains_key(http::header::SET_COOKIE),
            "the upstream's session is the gateway's, not the client's"
        );
        assert!(!headers.contains_key(http::header::CONNECTION));
        assert!(!headers.contains_key("transfer-encoding"));
        assert!(
            !headers.contains_key("x-upstream-token"),
            "a header the upstream nominated in `Connection` is its own, not the client's"
        );
        assert_eq!(headers["content-type"], "application/json");
    }
}
