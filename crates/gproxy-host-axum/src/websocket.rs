//! WebSocket upgrades: the handshake, the duplex pump and what a socket holds
//! open while it runs.
//!
//! # Nothing is upgraded before it is allowed
//!
//! An accepted socket is an answer. A client that sees `101` believes it is
//! talking to a model, and a socket that is accepted and then immediately
//! closed tells it nothing about *why* — there is no status, no body and no
//! error code once the protocol has switched. So the whole of the decision
//! happens while this is still an HTTP request: the caller is authenticated,
//! the handshake shape is checked, admission runs, and the upstream handshake
//! is performed. Only when all four have succeeded is
//! [`WebSocketUpgrade::on_upgrade`] called.
//!
//! That order also decides the one rule this module exists to honour:
//!
//! > **A refused upstream handshake reaches the client as the upstream's own
//! > HTTP response, body and all.**
//!
//! [`UpstreamConnection::Rejected`] carries a whole `WireResponse` for exactly
//! this reason — "the complete response body is retained so the host can
//! decode or relay the vendor error" — and a vendor's `403 {"error": {"code":
//! "insufficient_quota"}}` is worth more to the caller than any 502 this
//! gateway could invent. See [`refused`] for the one thing this host does to
//! that response, and why.
//!
//! # A socket holds its lease for as long as it is open
//!
//! The rule is the crate's, not this module's: an `Admitted` measures requests
//! *in flight* and a realtime session legitimately runs for an hour. So the
//! pump owns a [`Trailer`] — the same value [`crate::response::LeasedBody`]
//! puts inside a response body — and releases it in the same order when the
//! socket ends: settle usage, write the capture, hand the permits back. There
//! is nowhere else the lease lives, so no path can forget it.
//!
//! # A client that vanishes cancels the upstream call
//!
//! The pump always closes the upstream socket, so a departed client has never
//! left a *socket* running. What it did leave is a request core still believes
//! in: the session's cancellation token, carried by the same [`Trailer`], is
//! what tells core the answer was abandoned rather than delivered, and a socket
//! settled as `Completed` when nobody was listening is a usage row that lies.
//!
//! So every exit from [`Pump::duplex`] that means *the client is gone* cancels
//! the token — its stream ending, its stream failing (a socket dropped without
//! a close frame arrives as `ResetWithoutClosingHandshake`, which is a failure
//! and not an end), and a send to it that could not be written. A **close
//! frame** from either side does not: that is a session two peers finished, it
//! settles as `Complete`, and cancelling would overwrite the truth.
//!
//! The cancel goes out *after* the close frame has gone upstream, so the
//! upstream is told politely first, and before [`Pump::drain_upstream`], whose
//! read is what lets core observe it.
//!
//! A service socket retains a capture-only trailer, without model usage or leases.
//!
//! # What is pumped
//!
//! Two independent sockets, terminated here. Text and binary messages cross in
//! both directions unchanged; a close on one side is forwarded to the other and
//! the closing handshake is then read to its end ([`Pump::drain_upstream`]);
//! ping and pong stay on the side they were sent on, because a keepalive
//! measures the link it travelled and forwarding one would measure the wrong
//! link — a slow upstream would start looking like a dead client. The client's
//! own ping is answered by the downstream codec (axum documents that it does);
//! the upstream's is answered here, because nothing else will.
//!
//! `max_ws_frame_bytes` is enforced on both directions. Core enforces it too,
//! on the socket it hands back, and that is not redundant: core's refusal is a
//! transport error that tears the session down, while a limit this host
//! notices closes both sides cleanly with `1009 Message Too Big`, which is
//! what a client can act on.
//!
//! # What is captured
//!
//! One `capture_records` row per socket, promoted to `WsConnection` by the
//! `101`, plus one `capture_events` row per message in the order it crossed.
//! `turn_id` is always unset and no `WsTurn` record is written; the reason is
//! [`DownstreamCapture::record_frame`]'s, and it is that a business turn is a
//! dialect's notion while this host forwards realtime frames opaquely.

use std::sync::Arc;

use axum::{
    extract::{
        FromRequestParts as _,
        ws::{CloseFrame, Message, WebSocket as ClientSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt as _, StreamExt as _};
use gproxy_app::{
    App, AppError, Caller, CaptureDirection, CaptureOutcome, CapturedFrame, ConnectOutcome,
    DataPlaneRequest, ServiceRequestIn,
};
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::UpstreamConnection,
    connection::{Bytes, WebSocket as UpstreamSocket, WsClose, WsFrame, WsReceiver, WsSender},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, StatusCode, header};

use crate::response::{CancelOnDrop, Trailer, passthrough, sanitize};

/// RFC 6455 §7.4.1 `1009`: the peer sent a message too big to process. What a
/// frame over `max_ws_frame_bytes` earns, on either side.
const CLOSE_TOO_BIG: u16 = 1009;
/// RFC 6455 §7.4.1 `1011`: the server hit a condition it cannot recover from.
/// What a transport failure on the other socket earns.
const CLOSE_INTERNAL: u16 = 1011;
/// RFC 6455 §7.4.1 `1000`: a normal closure, for the side that has to be told
/// the other one went away without saying anything.
const CLOSE_NORMAL: u16 = 1000;

/// How long the upstream is given to answer this gateway's close frame before
/// its socket is dropped. Short, because the session is over either way and
/// the only thing still waiting is the request slot it is holding.
const CLOSE_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Take the upgrade out of the request, or say why this is not one.
///
/// Runs **after** authentication and before anything is accepted: an
/// anonymous caller learns nothing about this gateway's transports, and a
/// client that did authenticate is told plainly what is wrong with its
/// handshake. Nothing is upgraded here — the `101` is only written by
/// [`WebSocketUpgrade::on_upgrade`] — so a request that is refused after this
/// point is still refused over HTTP.
///
/// `max_frame_bytes` becomes the codec's message cap, one byte over the
/// configured limit, so a frame exactly at the limit is accepted and one over
/// it is caught by this host (with a close code the client can read) rather
/// than by tungstenite (with a protocol error it cannot). The codec is still
/// the memory bound: nothing larger than that is ever buffered.
pub async fn extract(
    parts: &mut http::request::Parts,
    max_frame_bytes: u64,
) -> Result<Upgrade, Box<Response>> {
    let upgrade = WebSocketUpgrade::from_request_parts(parts, &())
        .await
        .map_err(|rejection| Box::new(reject(rejection)))?;
    let max = usize::try_from(max_frame_bytes.saturating_add(1)).unwrap_or(usize::MAX);
    Ok(Upgrade(upgrade.max_message_size(max).max_frame_size(max)))
}

/// The extracted handshake, so a caller cannot confuse it with anything else
/// and cannot accidentally accept it without a decision.
pub struct Upgrade(WebSocketUpgrade);

impl std::fmt::Debug for Upgrade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Upgrade { .. }")
    }
}

/// Step 5, for the surfaces that are a handshake: admit, connect, upgrade.
///
/// The three refusals answer over HTTP, in this order: admission
/// ([`App::connect`] returns `Forbidden`, `RateLimited`, `NotFound`), the
/// engine (no usable credential, a transform that cannot be done over a
/// socket), and the upstream's own refusal, which is relayed as it stands.
///
/// `cancel` is the request's token guard, held across the handshake — a client
/// that gives up while this is still deciding drops this future — and then moved
/// into the pump for the life of the socket.
pub async fn data_plane<C>(
    app: Arc<App<C>>,
    upgrade: Upgrade,
    caller: &Caller,
    request: DataPlaneRequest,
    max_frame_bytes: u64,
    mut cancel: CancelOnDrop,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let outcome = match app.connect(caller, request).await {
        Ok(outcome) => outcome,
        Err(error) => {
            // Same as the HTTP path: `App::connect` has already released
            // everything it charged, so there is nothing left to cancel.
            cancel.disarm();
            return crate::ErrorResponse(error).into_response();
        }
    };
    let ConnectOutcome {
        execution,
        admitted,
        capture,
    } = outcome;
    let (connection, usage) = execution.into_parts();
    let mut trailer = Trailer::new(app, admitted, capture, usage, cancel);

    match connection {
        // The rule this module exists for. The vendor's status, headers and
        // body are the answer.
        UpstreamConnection::Rejected(response) => refused(trailer, response).await,
        UpstreamConnection::Connected { handshake, socket } => {
            // What the client is told besides the four headers axum computes
            // for itself (`connection`, `upgrade`, `sec-websocket-accept` and
            // the selected subprotocol). Everything hop-by-hop is dropped:
            // `sec-websocket-accept` in particular proves the *upstream's*
            // handshake and says nothing about the client's.
            let negotiated = sanitize(handshake.headers);
            if let Some(capture) = trailer.capture_mut() {
                // `101` is what turns the record into a `WsConnection`. The
                // headers are the upstream's negotiated ones, which are the
                // whole informative content of the answer the client gets.
                capture.record_response_head(StatusCode::SWITCHING_PROTOCOLS, &negotiated);
            }
            accept(upgrade, socket, max_frame_bytes, Some(trailer), &negotiated)
        }
    }
}

/// The upstream's refusal, relayed whole.
///
/// **The one place this host buffers a response body on purpose.** A client
/// that is in the middle of a handshake is not an HTTP client yet: it read the
/// status line expecting `101`, and every websocket library then reads what is
/// left of the response out of the same buffer. A chunked body — which is what
/// a streamed relay produces, since the upstream's `transfer-encoding` is
/// hop-by-hop and stripped — arrives at such a client with its chunk framing
/// still in it, and the vendor's JSON becomes unparseable. So the body is
/// collected, the length is stated, and the whole answer goes out in one
/// piece.
///
/// Buffering also settles the request here rather than on the last byte a
/// client may never read: a handshake client that sees a non-`101` typically
/// drops the connection, and a lease released by the drop path loses the
/// capture and the usage row with it.
async fn refused<C>(mut trailer: Trailer<C>, response: WireResponse<HttpBody>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let WireResponse {
        status,
        headers,
        body,
    } = response;
    let (body, complete) = collect(body).await;
    if let Some(capture) = trailer.capture_mut() {
        capture.record_response_head(status, &headers);
        capture.record_response_chunk(&body);
    }
    trailer
        .settle(if complete {
            CaptureOutcome::Complete
        } else {
            CaptureOutcome::Interrupted
        })
        .await;
    relay(status, headers, body)
}

/// One buffered upstream response, framed by its length.
fn relay(status: StatusCode, headers: HeaderMap, body: Bytes) -> Response {
    let length = body.len() as u64;
    let mut response = passthrough(status, headers, HttpBody::Bytes(body));
    // Stated rather than inherited: the upstream's own `content-length`
    // described its framing, and a body that was chunked had none at all.
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, http::HeaderValue::from(length));
    response
}

/// A body as bytes, and whether it ended cleanly. A transport failure part way
/// through keeps what arrived: a truncated vendor error is still more than a
/// gateway's guess at one.
async fn collect(body: HttpBody) -> (Bytes, bool) {
    match body {
        HttpBody::Bytes(bytes) => (bytes, true),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(chunk) => {
                        if out.len().saturating_add(chunk.len()) > crate::MAX_BODY_BYTES {
                            return (Bytes::from(out), false);
                        }
                        out.extend_from_slice(&chunk);
                    }
                    Err(error) => {
                        tracing::debug!(%error, "a refused handshake's body was cut short");
                        return (Bytes::from(out), false);
                    }
                }
            }
            (Bytes::from(out), true)
        }
    }
}

/// Step 4, for a channel's vendor service that is a socket — Codex's
/// remote-control server is the one in the tree.
///
/// A service records its downstream handshake and frames without acquiring a
/// model-call lease or settling usage. The capture-only trailer is finalized
/// by the same pump as model traffic; the core service API has no cancellation token.
///
/// Which credential the socket may speak for is the channel's: the Codex
/// channel refuses a synthesized view outright, so `x-gproxy-view:
/// credential:{id}` — and an administrator of that credential — is the only way
/// through.
pub async fn service<C>(
    app: &Arc<App<C>>,
    upgrade: Upgrade,
    caller: &Caller,
    request: ServiceRequestIn,
    max_frame_bytes: u64,
    capture: Option<gproxy_app::capture::DownstreamCapture>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let mut trailer = Trailer::service(app.clone(), capture);
    match app.connect_service(caller, request).await {
        Err(error) => {
            let response = crate::ErrorResponse(error).into_response();
            if let Some(capture) = trailer.capture_mut() {
                capture.record_response_head(response.status(), response.headers());
            }
            trailer.settle(CaptureOutcome::Interrupted).await;
            response
        }
        Ok(UpstreamConnection::Rejected(response)) => refused(trailer, response).await,
        Ok(UpstreamConnection::Connected { handshake, socket }) => {
            let negotiated = sanitize(handshake.headers);
            if let Some(capture) = trailer.capture_mut() {
                capture.record_response_head(StatusCode::SWITCHING_PROTOCOLS, &negotiated);
            }
            accept::<C>(upgrade, socket, max_frame_bytes, Some(trailer), &negotiated)
        }
    }
}

/// Write the `101` and start the pump.
///
/// The upstream's negotiated headers are appended to axum's response without
/// overwriting any of its own: a subprotocol the upstream chose has to reach
/// the client, and `sec-websocket-accept` must stay the one computed from the
/// client's key.
fn accept<C>(
    upgrade: Upgrade,
    upstream: UpstreamSocket,
    max_frame_bytes: u64,
    trailer: Option<Trailer<C>>,
    negotiated: &HeaderMap,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let mut response = upgrade.0.on_upgrade(move |downstream| {
        Pump {
            downstream,
            incoming: upstream.incoming,
            outgoing: upstream.outgoing,
            limit: max_frame_bytes,
            trailer,
        }
        .run()
    });
    let headers = response.headers_mut();
    for name in negotiated.keys() {
        if headers.contains_key(name) {
            continue;
        }
        for value in negotiated.get_all(name) {
            headers.append(name, value.clone());
        }
    }
    response
}

/// How one side's message left the loop.
enum Step {
    /// Keep pumping.
    Go,
    /// The socket is over; this is what the record should say.
    Stop(CaptureOutcome),
}

/// Both halves of one session and everything it holds open.
struct Pump<C> {
    downstream: ClientSocket,
    incoming: WsReceiver,
    outgoing: WsSender,
    limit: u64,
    /// Owns model usage/leases for model traffic, capture only for services.
    trailer: Option<Trailer<C>>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Pump<C> {
    /// Forward frames until one side stops, then finish the closing handshake
    /// and settle.
    async fn run(mut self) {
        let outcome = self.duplex().await;
        self.drain_upstream().await;
        let Self {
            mut downstream,
            incoming,
            mut outgoing,
            trailer,
            ..
        } = self;
        let _ = outgoing.close().await;
        let _ = downstream.flush().await;
        // **Both halves go before the settlement is awaited.** Core arms the
        // exchange's settlement inside the socket it handed over — the guard
        // that finishes it lives in the incoming stream — so awaiting the
        // `UsageCompletion` while still holding the socket waits for a value
        // only dropping the socket can produce.
        drop((incoming, outgoing, downstream));
        if let Some(trailer) = trailer {
            // Still armed until here, so a pump whose task is dropped outright
            // — hyper tearing the connection down — cancels rather than leaving
            // core waiting. `settle` disarms it before awaiting the settlement.
            trailer.settle(outcome).await;
        }
    }

    /// The other half of RFC 6455's closing handshake.
    ///
    /// Every exit from [`Pump::duplex`] has either sent this gateway's close
    /// upstream or seen the upstream's own, so what is left is the answer. It
    /// is read to the end rather than dropped, and not out of politeness: core
    /// finishes the exchange when its stream ends, so a socket dropped
    /// mid-handshake is metered as interrupted even though it closed cleanly.
    /// Frames that arrive during the drain are discarded — the client has
    /// already been told the socket is over.
    ///
    /// A peer that never answers gets [`CLOSE_GRACE`] and is then dropped,
    /// because a gateway cannot let one unresponsive upstream hold a request
    /// slot for ever.
    async fn drain_upstream(&mut self) {
        let drain = async { while self.incoming.next().await.is_some() {} };
        if tokio::time::timeout(CLOSE_GRACE, drain).await.is_err() {
            tracing::debug!("the upstream did not answer the close within the grace period");
        }
    }

    async fn duplex(&mut self) -> CaptureOutcome {
        loop {
            let step = tokio::select! {
                message = self.downstream.recv() => match message {
                    Some(Ok(message)) => self.client_message(message).await,
                    // A client that hung up without a close frame. The
                    // upstream is told so the session stops costing money.
                    None => {
                        self.close_upstream(None).await;
                        self.client_gone();
                        Step::Stop(CaptureOutcome::Cancelled)
                    }
                    Some(Err(error)) => {
                        tracing::debug!(%error, "client socket failed");
                        self.close_upstream(Some(WsClose {
                            code: CLOSE_INTERNAL,
                            reason: String::new(),
                        }))
                        .await;
                        // Still a client that is gone, and the usual one: a
                        // dropped socket reaches us as
                        // `ResetWithoutClosingHandshake` rather than as an end
                        // of stream. The capture records the transport failure
                        // it was, and the request is cancelled because nobody
                        // is going to read the rest of it.
                        self.client_gone();
                        Step::Stop(CaptureOutcome::Interrupted)
                    }
                },
                frame = self.incoming.next() => match frame {
                    Some(Ok(frame)) => self.upstream_frame(frame).await,
                    None => {
                        self.close_client(CLOSE_NORMAL, "upstream closed").await;
                        Step::Stop(CaptureOutcome::Complete)
                    }
                    Some(Err(error)) => {
                        tracing::debug!(%error, "upstream socket failed");
                        self.close_client(CLOSE_INTERNAL, "upstream failed").await;
                        Step::Stop(CaptureOutcome::Interrupted)
                    }
                },
            };
            if let Step::Stop(outcome) = step {
                return outcome;
            }
        }
    }

    /// One message from the client, on its way upstream.
    async fn client_message(&mut self, message: Message) -> Step {
        let frame = match message {
            Message::Text(text) => WsFrame::Text(text.as_str().to_owned()),
            Message::Binary(bytes) => WsFrame::Binary(bytes),
            Message::Ping(payload) => {
                // Answered by the downstream codec, not forwarded: it measures
                // the client's link to this gateway.
                self.record(CaptureDirection::Request, &WsFrame::Ping(payload));
                return Step::Go;
            }
            Message::Pong(payload) => {
                self.record(CaptureDirection::Request, &WsFrame::Pong(payload));
                return Step::Go;
            }
            Message::Close(frame) => {
                let close = frame.map(|frame| WsClose {
                    code: frame.code,
                    reason: frame.reason.as_str().to_owned(),
                });
                self.close_upstream(close).await;
                return Step::Stop(CaptureOutcome::Complete);
            }
        };
        if frame_len(&frame) as u64 > self.limit {
            return self.too_big(CaptureDirection::Request).await;
        }
        self.record(CaptureDirection::Request, &frame);
        if let Err(error) = self.outgoing.send(frame).await {
            tracing::debug!(%error, "upstream send failed");
            self.close_client(CLOSE_INTERNAL, "upstream failed").await;
            return Step::Stop(CaptureOutcome::Interrupted);
        }
        Step::Go
    }

    /// One frame from the upstream, on its way to the client.
    async fn upstream_frame(&mut self, frame: WsFrame) -> Step {
        if frame_len(&frame) as u64 > self.limit {
            return self.too_big(CaptureDirection::Response).await;
        }
        self.record(CaptureDirection::Response, &frame);
        let message = match frame {
            WsFrame::Text(text) => Message::Text(text.into()),
            WsFrame::Binary(bytes) => Message::Binary(bytes),
            WsFrame::Ping(payload) => {
                // Answered upstream rather than forwarded, for the same reason
                // the client's ping stays downstream — and here it has to be
                // this host, since core's socket is a raw duplex with no
                // keepalive courtesy of its own.
                if self.outgoing.send(WsFrame::Pong(payload)).await.is_err() {
                    self.close_client(CLOSE_INTERNAL, "upstream failed").await;
                    return Step::Stop(CaptureOutcome::Interrupted);
                }
                return Step::Go;
            }
            WsFrame::Pong(_) => return Step::Go,
            WsFrame::Close(close) => {
                let (code, reason) = close
                    .as_ref()
                    .map(|close| (close.code, close.reason.clone()))
                    .unwrap_or((CLOSE_NORMAL, String::new()));
                // RFC 6455 §5.5.1: a close is answered with a close, echoing
                // the code. The codec under core's socket may well answer for
                // us, and a second close frame after one has been sent is
                // ignored; doing it here makes the closing handshake this
                // module's behaviour rather than whichever library is
                // underneath, and it is what lets `drain_upstream` finish.
                //
                // Both sends relay: the close was recorded above, on arrival.
                self.relay_close_upstream(close).await;
                self.relay_close_to_client(code, &reason).await;
                return Step::Stop(CaptureOutcome::Complete);
            }
        };
        if let Err(error) = self.downstream.send(message).await {
            tracing::debug!(%error, "client send failed");
            self.close_upstream(None).await;
            self.client_gone();
            return Step::Stop(CaptureOutcome::Cancelled);
        }
        Step::Go
    }

    /// The client is gone. Cancel the request's token, which is the only thing
    /// that tells core the session was abandoned rather than finished: it reads
    /// the token when it chooses between `Completed` and `Cancelled`, and its
    /// half of the socket selects on it, so the exchange ends here rather than
    /// whenever the upstream next says something.
    ///
    /// Called *after* the close frame has gone upstream — cancelling first would
    /// not lose the frame, since core's sink ignores the token, but the polite
    /// order is the one worth having — and before [`Pump::drain_upstream`], whose
    /// read is what lets core observe it. A service socket has no token and this
    /// is a no-op.
    fn client_gone(&mut self) {
        if let Some(trailer) = self.trailer.as_mut() {
            trailer.cancel();
        }
    }

    /// A frame over `max_ws_frame_bytes`, refused on both sides with the code
    /// RFC 6455 reserves for it. The oversized payload is never recorded: it
    /// is the thing the limit exists to keep out of memory.
    async fn too_big(&mut self, direction: CaptureDirection) -> Step {
        let reason = match direction {
            CaptureDirection::Request => "client frame exceeds the frame limit",
            CaptureDirection::Response => "upstream frame exceeds the frame limit",
        };
        tracing::debug!(reason, limit = self.limit, "websocket frame refused");
        self.close_client(CLOSE_TOO_BIG, reason).await;
        self.close_upstream(Some(WsClose {
            code: CLOSE_TOO_BIG,
            reason: reason.to_owned(),
        }))
        .await;
        Step::Stop(CaptureOutcome::Interrupted)
    }

    /// A close this host decided on, recorded and sent to the client.
    ///
    /// **A close is recorded exactly once, in the direction it travelled, by
    /// whichever side first saw it.** So a close the gateway invents goes
    /// through here, and one it is merely passing on uses
    /// [`Pump::relay_close_to_client`] instead — it was already recorded when
    /// it arrived, and a log with the same close in it twice would suggest two.
    async fn close_client(&mut self, code: u16, reason: &str) {
        self.record(
            CaptureDirection::Response,
            &WsFrame::Close(Some(WsClose {
                code,
                reason: reason.to_owned(),
            })),
        );
        self.relay_close_to_client(code, reason).await;
    }

    async fn relay_close_to_client(&mut self, code: u16, reason: &str) {
        let frame = CloseFrame {
            code,
            reason: reason.to_owned().into(),
        };
        let _ = self.downstream.send(Message::Close(Some(frame))).await;
    }

    /// The same for the upstream's side.
    async fn close_upstream(&mut self, close: Option<WsClose>) {
        self.record(CaptureDirection::Request, &WsFrame::Close(close.clone()));
        self.relay_close_upstream(close).await;
    }

    async fn relay_close_upstream(&mut self, close: Option<WsClose>) {
        let _ = self.outgoing.send(WsFrame::Close(close)).await;
    }

    /// Hand the frame to the downstream capture, if there is one. A no-op
    /// with `enable_downstream_log_body` off, which the capture decides.
    fn record(&mut self, direction: CaptureDirection, frame: &WsFrame) {
        let Some(capture) = self.trailer.as_mut().and_then(Trailer::capture_mut) else {
            return;
        };
        capture.record_frame(direction, captured(frame));
    }
}

/// A protocol frame as the capture names it.
fn captured(frame: &WsFrame) -> CapturedFrame<'_> {
    match frame {
        WsFrame::Text(text) => CapturedFrame::Text(text),
        WsFrame::Binary(bytes) => CapturedFrame::Binary(bytes),
        WsFrame::Ping(bytes) => CapturedFrame::Ping(bytes),
        WsFrame::Pong(bytes) => CapturedFrame::Pong(bytes),
        WsFrame::Close(close) => CapturedFrame::Close {
            code: close.as_ref().map(|close| close.code),
            reason: close.as_ref().map_or("", |close| close.reason.as_str()),
        },
    }
}

/// Payload bytes in one frame, counted the way core counts them: a close frame
/// is control, not payload, so it is never over the limit.
fn frame_len(frame: &WsFrame) -> usize {
    match frame {
        WsFrame::Text(text) => text.len(),
        WsFrame::Binary(bytes) | WsFrame::Ping(bytes) | WsFrame::Pong(bytes) => bytes.len(),
        WsFrame::Close(_) => 0,
    }
}

/// `426`, with the header that says what to upgrade to. For a path that is a
/// declared handshake surface reached without a handshake.
pub(crate) fn upgrade_required(message: impl Into<String>) -> Response {
    let mut response = (StatusCode::UPGRADE_REQUIRED, message.into()).into_response();
    response
        .headers_mut()
        .insert(header::UPGRADE, http::HeaderValue::from_static("websocket"));
    response
}

/// The refusal a malformed handshake earns.
///
/// Axum's own rejections render as `405`, `400` and `426`; this maps them onto
/// the two answers a client can act on, in this crate's error envelope:
///
/// - **`426`** when the request is not an upgrade this connection can serve —
///   the wrong method, no `Connection: upgrade`, no `Upgrade: websocket`, or a
///   connection that cannot be upgraded at all (HTTP/1.0, or HTTP/2 without
///   extended CONNECT). All of those are answered by *re-sending* the request
///   as a handshake over HTTP/1.1, which is exactly what `426` plus
///   `Upgrade: websocket` tells a client to do. A `405` would be wrong:
///   the path is not method-restricted, it is transport-restricted.
/// - **`400`** when it *is* a handshake and is malformed — a missing
///   `Sec-WebSocket-Key`, a version that is not 13. Re-sending it unchanged
///   will not help.
fn reject(rejection: axum::extract::ws::rejection::WebSocketUpgradeRejection) -> Response {
    use axum::extract::ws::rejection::WebSocketUpgradeRejection as R;
    match rejection {
        R::MethodNotGet(_)
        | R::MethodNotConnect(_)
        | R::InvalidConnectionHeader(_)
        | R::InvalidUpgradeHeader(_)
        | R::InvalidProtocolPseudoheader(_)
        | R::ConnectionNotUpgradable(_) => {
            upgrade_required("this surface is a websocket; send an Upgrade request")
        }
        other => crate::ErrorResponse(AppError::invalid(format!(
            "websocket handshake: {}",
            other.body_text()
        )))
        .into_response(),
    }
}
