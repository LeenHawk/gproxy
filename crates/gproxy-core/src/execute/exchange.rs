//! One physical upstream exchange and the client wrapper that observes it.
//! The capture and reason taps borrow bytes as they pass; nothing is copied
//! unless the policy asked for it. Usage is not read here: the upstream's
//! bytes are the vendor's, and usage is read from the standard response the
//! channel shapes out of them (`metering`). A websocket session, whose frames
//! pass through unshaped, is the exception and is read on its exchange.

use super::{
    Funnel,
    metering::{Reading, UsageGate},
    stream::observed_body,
};
use crate::{
    AttemptContext, CaptureEvent, CapturePolicy, CaptureSink, ExchangeContext, ExchangeUsage,
    RewriteRuleData, TraceEvent, UsageState, convert::response_framing, ids,
};
use gproxy_channel::{
    BaseChannel, OutboundClient,
    channel::{NormalizedUsage, ResponseReason, ResponseReasonObserver, UsageStreamEnd},
};
use gproxy_protocol::{
    HttpBody, OperationKey, WireResponse,
    capability::{CapabilityError, CapabilityFuture, CapabilityLimits, UpstreamConnection},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering},
};

pub(crate) struct Exchange {
    pub context: Arc<ExchangeContext>,
    pub funnel: Arc<Funnel>,
    pub channel: Arc<dyn BaseChannel>,
    pub limits: CapabilityLimits,
    /// Body response rules already selected for this request.
    pub response_rules: Vec<Arc<RewriteRuleData>>,
    capture: Mutex<Option<Box<dyn CaptureSink>>>,
    reason_observer: Mutex<Option<Box<dyn ResponseReasonObserver>>>,
    reason: Mutex<Option<ResponseReason>>,
    sequence: AtomicU64,
    /// The usage of a websocket session, read from its frames.
    pub(super) ws_reading: Mutex<Option<Reading>>,
    /// The native call this exchange serves, when that call is metered: its
    /// reading reaches this exchange's capture through the gate.
    gate: Mutex<Option<Arc<UsageGate>>>,
    /// Ending this exchange's body ends the request.
    terminal: AtomicBool,
    finished: AtomicBool,
    /// The caller let go of the body/socket before it ended.
    dropped: AtomicBool,
    /// Response body bytes received, which tell the send that answered a
    /// native call from the sends around it.
    received: Arc<AtomicU64>,
    status: AtomicU16,
    claimed: AtomicBool,
    closed: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl Exchange {
    pub fn new(
        funnel: Arc<Funnel>,
        attempt: Arc<AttemptContext>,
        operation: OperationKey,
        channel: Arc<dyn BaseChannel>,
        response_rules: Vec<Arc<RewriteRuleData>>,
        limits: CapabilityLimits,
        now_ms: i64,
    ) -> Arc<Self> {
        let context = Arc::new(ExchangeContext {
            capture_id: format!("{}-{}", attempt.attempt_id, ids::random_id()),
            attempt,
            operation,
            started_at_ms: now_ms,
        });
        Arc::new(Self {
            context,
            funnel,
            channel,
            limits,
            response_rules,
            capture: Mutex::new(None),
            reason_observer: Mutex::new(None),
            reason: Mutex::new(None),
            sequence: AtomicU64::new(0),
            ws_reading: Mutex::new(None),
            gate: Mutex::new(None),
            terminal: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            dropped: AtomicBool::new(false),
            received: Arc::new(AtomicU64::new(0)),
            status: AtomicU16::new(0),
            claimed: AtomicBool::new(false),
            closed: Mutex::new(None),
        })
    }

    /// Channels may issue several real sends during one operation invocation.
    /// Reuse the first context, then give each additional call its own identity.
    /// Every send is recorded with the call's gate, so the one that answered
    /// can be told apart from a bootstrap or a token exchange before it.
    fn for_send(self: &Arc<Self>) -> Arc<Self> {
        let exchange = if self.claimed.swap(true, Ordering::SeqCst) {
            let exchange = Self::new(
                self.funnel.clone(),
                self.context.attempt.clone(),
                self.context.operation,
                self.channel.clone(),
                self.response_rules.clone(),
                self.limits,
                crate::api::lifecycle::now_ms(),
            );
            *exchange.gate.lock().unwrap() = self.gate.lock().unwrap().clone();
            exchange
        } else {
            self.clone()
        };
        if let Some(gate) = exchange.gate.lock().unwrap().as_ref() {
            gate.started(&exchange.context.capture_id, exchange.received.clone());
        }
        exchange
    }

    /// Tie this exchange, and every send made through it, to a metered
    /// native call.
    pub(super) fn meter_through(&self, gate: Arc<UsageGate>) {
        *self.gate.lock().unwrap() = Some(gate);
    }

    /// Mark that the caller receives this exchange's body: its end settles the request.
    pub fn make_terminal(&self) {
        self.terminal.store(true, Ordering::SeqCst);
    }

    pub fn wants_full_capture(&self) -> bool {
        self.funnel.policy().capture == CapturePolicy::Full
    }

    pub fn record(&self, event: CaptureEvent<'_>) {
        if matches!(&event, CaptureEvent::RequestHead { .. }) {
            *self.closed.lock().unwrap() = Some(self.funnel.open_exchange());
            let policy = self.funnel.policy().capture;
            if policy != CapturePolicy::Off {
                *self.capture.lock().unwrap() =
                    Some(self.funnel.observer().capture(&self.context, policy));
            }
            self.funnel
                .trace(TraceEvent::ExchangeStarted(&self.context));
        }
        if let CaptureEvent::ResponseHead { status, headers } = &event {
            self.status.store(status.as_u16(), Ordering::Relaxed);
            if self.funnel.policy().capture != CapturePolicy::Off {
                *self.reason_observer.lock().unwrap() =
                    self.channel
                        .response_reason_observer(*status, headers, self.limits.read_bytes);
            }
        }
        if let Some(sink) = self.capture.lock().unwrap().as_mut() {
            let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
            sink.record(sequence, event);
        }
    }

    pub fn set_reason(&self, reason: ResponseReason) {
        let mut current = self.reason.lock().unwrap();
        if *current == Some(reason) {
            return;
        }
        *current = Some(reason);
        drop(current);
        // Long-lived sockets must expose a discovered reason before they close.
        if let Some(sink) = self.capture.lock().unwrap().as_mut() {
            sink.reason(reason);
        }
    }

    pub fn observe_chunk(&self, chunk: &[u8]) {
        if let Some(observer) = self.reason_observer.lock().unwrap().as_mut() {
            observer.observe(chunk);
        }
        self.received
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }

    fn report_usage(&self, usage: Option<NormalizedUsage>) {
        let Some(usage) = usage else {
            return;
        };
        let attempt = &self.context.attempt;
        self.funnel.record_exchange_usage(ExchangeUsage {
            capture_id: self.context.capture_id.clone(),
            attempt_id: attempt.attempt_id.clone(),
            attempt_ordinal: attempt.ordinal,
            provider_id: attempt.credential.provider_id.clone(),
            credential_id: attempt.credential.id.clone(),
            upstream_model: attempt.request.target.upstream_model.clone(),
            usage,
            cost: None,
        });
    }

    /// Usage and trace for the end of this exchange, synchronously: the drop
    /// path must record usage before anything else can settle the request.
    /// Returns the capture sink still to be closed, or None when already done.
    fn settle_sync(
        &self,
        end: UsageStreamEnd,
        status: Option<http::StatusCode>,
        now_ms: i64,
    ) -> Option<Option<Box<dyn CaptureSink>>> {
        if self.finished.swap(true, Ordering::SeqCst) {
            return None;
        }
        let reason = self
            .reason_observer
            .lock()
            .unwrap()
            .take()
            .and_then(|o| o.finish())
            .or(*self.reason.lock().unwrap())
            .or_else(|| match self.status.load(Ordering::Relaxed) {
                400 | 422 => Some(ResponseReason::InvalidRequest),
                401 => Some(ResponseReason::AuthenticationFailed),
                402 => Some(ResponseReason::QuotaExhausted),
                403 => Some(ResponseReason::PermissionDenied),
                404 => Some(ResponseReason::NotFound),
                408 | 504 => Some(ResponseReason::Timeout),
                429 => Some(ResponseReason::RateLimited),
                500..=599 => Some(ResponseReason::UpstreamError),
                _ => None,
            });
        if let Some(reason) = reason
            && let Some(sink) = self.capture.lock().unwrap().as_mut()
        {
            sink.reason(reason);
        }
        // Only a websocket session is read here; an HTTP exchange's usage
        // comes from its native call's shaped response, through the gate.
        let reading = self.ws_reading.lock().unwrap().take();
        let usage = reading.and_then(|reading| reading.finish(end));
        if let Some(usage) = usage.as_ref()
            && let Some(sink) = self.capture.lock().unwrap().as_mut()
        {
            sink.usage(usage);
        }
        self.report_usage(usage);
        self.funnel.trace(TraceEvent::ExchangeFinished {
            exchange: &self.context,
            status,
            finished_at_ms: now_ms,
        });
        Some(self.capture.lock().unwrap().take())
    }

    fn terminal_state(&self, end: UsageStreamEnd) -> Option<UsageState> {
        if !self.terminal.load(Ordering::SeqCst) {
            return None;
        }
        Some(match end {
            UsageStreamEnd::Complete if self.status.load(Ordering::Relaxed) >= 400 => {
                UsageState::Failed
            }
            UsageStreamEnd::Complete => UsageState::Completed,
            UsageStreamEnd::Interrupted => {
                if self.dropped.load(Ordering::SeqCst)
                    || self.context.attempt.request.cancellation.is_cancelled()
                {
                    UsageState::Cancelled
                } else {
                    UsageState::Failed
                }
            }
        })
    }

    /// Close capture and settle a terminal exchange. A metered call's
    /// capture first waits for the call's reading, which may still be on its
    /// way: the shaped response is read after the upstream's bytes end. That
    /// wait is not awaited here, since the shaped response may be waiting on
    /// this very exchange to finish; settlement waits for it instead, through
    /// the exchange's `closed` signal.
    async fn close(&self, sink: Option<Box<dyn CaptureSink>>, end: UsageStreamEnd) {
        let capture_end = match end {
            UsageStreamEnd::Complete => crate::CaptureEnd::Complete,
            UsageStreamEnd::Interrupted
                if (self.dropped.load(Ordering::SeqCst) && self.funnel.handed_off())
                    || self.context.attempt.request.cancellation.is_cancelled() =>
            {
                crate::CaptureEnd::Cancelled
            }
            UsageStreamEnd::Interrupted => crate::CaptureEnd::Interrupted,
        };
        let closed = self.closed.lock().unwrap().take();
        let state = self.terminal_state(end);
        let funnel = self.funnel.clone();
        let gate = self.gate.lock().unwrap().clone();
        let capture_id = self.context.capture_id.clone();
        let gated = gate.is_some();
        let (tx, rx) = tokio::sync::oneshot::channel();
        crate::rt::spawn(async move {
            if let Some(mut sink) = sink {
                if let Some(gate) = gate
                    && let Some(usage) = gate.wait(&capture_id).await
                {
                    sink.usage(&usage);
                }
                sink.finish(capture_end).await;
            }
            if let Some(closed) = closed {
                let _ = closed.send(());
            }
            if let Some(state) = state {
                funnel.finish(state).await;
            }
            let _ = tx.send(());
        });
        if !gated {
            let _ = rx.await;
        }
    }

    /// End of this exchange: finish a websocket's usage, close capture, and
    /// settle the request if this exchange was terminal. Idempotent.
    pub async fn finish(&self, end: UsageStreamEnd, status: Option<http::StatusCode>, now_ms: i64) {
        if let Some(sink) = self.settle_sync(end, status, now_ms) {
            self.close(sink, end).await;
        }
    }

    /// Drop path: usage is recorded right here, so a native body a conversion
    /// let go of after its terminal event still counts even when the client
    /// stream settles a moment later. Closing capture and settling a terminal
    /// exchange need awaits and go to the runtime; a dropped terminal exchange
    /// settles as Cancelled: the caller went away.
    pub fn finish_detached(
        self: Arc<Self>,
        end: UsageStreamEnd,
        status: http::StatusCode,
        now_ms: i64,
    ) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }
        self.dropped.store(true, Ordering::SeqCst);
        let Some(sink) = self.settle_sync(end, Some(status), now_ms) else {
            return;
        };
        let this = self.clone();
        if !crate::rt::spawn(async move {
            this.close(sink, end).await;
        }) && let Some(state) = self.terminal_state(end)
        {
            // No runtime to run the funnel on; at least resolve the completion.
            self.funnel.clone().finish_detached(state);
        }
    }
}

/// Wraps the credential's client for one exchange. Every request the channel
/// issues through it is captured; every response body is observed.
pub(crate) struct ObservedClient {
    inner: Arc<dyn OutboundClient>,
    exchange: Arc<Exchange>,
}

impl ObservedClient {
    pub fn new(inner: Arc<dyn OutboundClient>, exchange: Arc<Exchange>) -> Self {
        Self { inner, exchange }
    }
}

impl OutboundClient for ObservedClient {
    fn send<'b>(
        &'b self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'b, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let exchange = self.exchange.for_send();
            let (parts, body) = request.into_parts();
            exchange.record(CaptureEvent::RequestHead {
                method: &parts.method,
                uri: &parts.uri,
                headers: &parts.headers,
            });
            let body = match body {
                HttpBody::Bytes(bytes) => {
                    if exchange.wants_full_capture() {
                        exchange.record(CaptureEvent::RequestChunk(&bytes));
                    }
                    HttpBody::Bytes(bytes)
                }
                HttpBody::Stream(stream) => {
                    if exchange.wants_full_capture() {
                        let tap = exchange.clone();
                        HttpBody::Stream(Box::pin(futures_util::StreamExt::inspect(
                            stream,
                            move |chunk| {
                                if let Ok(chunk) = chunk {
                                    tap.record(CaptureEvent::RequestChunk(chunk));
                                }
                            },
                        )))
                    } else {
                        HttpBody::Stream(stream)
                    }
                }
            };
            let request = http::Request::from_parts(parts, body);
            let response = match self.inner.send(request).await {
                Ok(response) => response,
                Err(error) => {
                    exchange.set_reason(ResponseReason::ConnectionError);
                    exchange
                        .finish(
                            UsageStreamEnd::Interrupted,
                            None,
                            crate::api::lifecycle::now_ms(),
                        )
                        .await;
                    return Err(error);
                }
            };
            exchange.record(CaptureEvent::ResponseHead {
                status: response.status,
                headers: &response.headers,
            });
            let framing = response_framing(exchange.context.operation, &response.headers);
            let status = response.status;
            let headers = response.headers.clone();
            let body = observed_body(exchange, response.body, framing, status);
            Ok(WireResponse {
                status,
                headers,
                body: HttpBody::Stream(body),
            })
        })
    }

    fn connect<'b>(
        &'b self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'b, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            let (parts, ()) = request.into_parts();
            self.exchange.record(CaptureEvent::RequestHead {
                method: &parts.method,
                uri: &parts.uri,
                headers: &parts.headers,
            });
            let request = http::Request::from_parts(parts, ());
            let connection = match self.inner.connect(request).await {
                Ok(connection) => connection,
                Err(error) => {
                    self.exchange.set_reason(ResponseReason::ConnectionError);
                    self.exchange
                        .finish(
                            UsageStreamEnd::Interrupted,
                            None,
                            crate::api::lifecycle::now_ms(),
                        )
                        .await;
                    return Err(error);
                }
            };
            match connection {
                UpstreamConnection::Connected { handshake, socket } => {
                    self.exchange.record(CaptureEvent::ResponseHead {
                        status: handshake.status,
                        headers: &handshake.headers,
                    });
                    Ok(UpstreamConnection::Connected { handshake, socket })
                }
                UpstreamConnection::Rejected(response) => {
                    self.exchange.record(CaptureEvent::ResponseHead {
                        status: response.status,
                        headers: &response.headers,
                    });
                    let status = response.status;
                    let headers = response.headers.clone();
                    let body = observed_body(self.exchange.clone(), response.body, None, status);
                    Ok(UpstreamConnection::Rejected(WireResponse {
                        status,
                        headers,
                        body: HttpBody::Stream(body),
                    }))
                }
            }
        })
    }
}

impl Exchange {
    /// Settle as abandoned: the send or handshake future was dropped before
    /// any response body guard took over, so the caller went away and there
    /// is no status to record. Idempotent with every other way to finish.
    pub(crate) fn abandon(&self) {
        self.dropped.store(true, Ordering::SeqCst);
        if let Some(sink) = self.settle_sync(
            UsageStreamEnd::Interrupted,
            None,
            crate::api::lifecycle::now_ms(),
        ) {
            let closed = self.closed.lock().unwrap().take();
            if sink.is_some() || closed.is_some() {
                let gate = self.gate.lock().unwrap().clone();
                let capture_id = self.context.capture_id.clone();
                crate::rt::spawn(async move {
                    if let Some(mut sink) = sink {
                        if let Some(gate) = gate
                            && let Some(usage) = gate.wait(&capture_id).await
                        {
                            sink.usage(&usage);
                        }
                        sink.finish(crate::CaptureEnd::Cancelled).await;
                    }
                    if let Some(closed) = closed {
                        let _ = closed.send(());
                    }
                });
            }
        }
    }
}

impl Drop for Exchange {
    fn drop(&mut self) {
        self.abandon();
    }
}
