//! One physical upstream exchange and the client wrapper that observes it.
//! Capture and usage taps borrow bytes as they pass; nothing is copied unless
//! the policy asked for it.

use super::{Funnel, stream::observed_body};
use crate::{
    AttemptContext, CaptureEvent, CapturePolicy, CaptureSink, ExchangeContext, ExchangeUsage,
    RewriteRuleData, TraceEvent, UsageState, convert::response_framing, ids,
};
use gproxy_channel::{
    BaseChannel, OutboundClient,
    channel::{
        NormalizedUsage, ResponseView, UsageContext, UsageObserver, UsageStreamContext,
        UsageStreamEnd, UsageTransport,
    },
};
use gproxy_protocol::{
    HttpBody, OperationKey, WireResponse,
    capability::{CapabilityError, CapabilityFuture, CapabilityLimits, UpstreamConnection},
    connection::Bytes,
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
    sequence: AtomicU64,
    request_body: Mutex<Option<Bytes>>,
    pub(super) usage_observer: Mutex<Option<Box<dyn UsageObserver>>>,
    /// Ending this exchange's body ends the request.
    terminal: AtomicBool,
    finished: AtomicBool,
    /// The caller let go of the body/socket before it ended.
    dropped: AtomicBool,
    /// Response body bytes seen so far, for output estimation.
    response_bytes: AtomicU64,
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
            sequence: AtomicU64::new(0),
            request_body: Mutex::new(None),
            usage_observer: Mutex::new(None),
            terminal: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            dropped: AtomicBool::new(false),
            response_bytes: AtomicU64::new(0),
            status: AtomicU16::new(0),
            claimed: AtomicBool::new(false),
            closed: Mutex::new(None),
        })
    }

    /// Channels may issue several real sends during one operation invocation.
    /// Reuse the first context, then give each additional call its own identity.
    fn for_send(self: &Arc<Self>) -> Arc<Self> {
        if !self.claimed.swap(true, Ordering::SeqCst) {
            return self.clone();
        }
        Self::new(
            self.funnel.clone(),
            self.context.attempt.clone(),
            self.context.operation,
            self.channel.clone(),
            self.response_rules.clone(),
            self.limits,
            crate::api::lifecycle::now_ms(),
        )
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
        if let CaptureEvent::ResponseHead { status, .. } = &event {
            self.status.store(status.as_u16(), Ordering::Relaxed);
        }
        if let Some(sink) = self.capture.lock().unwrap().as_mut() {
            let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
            sink.record(sequence, event);
        }
    }

    pub fn observe_chunk(&self, chunk: &[u8]) {
        self.response_bytes
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        if let Some(observer) = self.usage_observer.lock().unwrap().as_mut() {
            // A failing observer only loses metering; the stream is untouched.
            let _ = observer.observe(gproxy_channel::channel::UsageFrame::HttpChunk(chunk));
        }
    }

    /// Whether the extractor path needs the whole response body accumulated.
    pub fn wants_accumulated_response(&self) -> bool {
        self.funnel.policy().usage
            && self.usage_observer.lock().unwrap().is_none()
            && self.channel.usage_extractor().is_some()
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
        headers: Option<&http::HeaderMap>,
        accumulated: Option<&[u8]>,
        now_ms: i64,
    ) -> Option<Option<Box<dyn CaptureSink>>> {
        if self.finished.swap(true, Ordering::SeqCst) {
            return None;
        }
        let observer = self.usage_observer.lock().unwrap().take();
        let mut usage = match observer {
            Some(observer) => observer.finish(end).ok().flatten(),
            None => {
                if let (Some(body), Some(status), Some(headers), Some(extractor)) =
                    (accumulated, status, headers, self.channel.usage_extractor())
                    && self.funnel.policy().usage
                {
                    let request_body = self.request_body.lock().unwrap().clone();
                    extractor
                        .extract(UsageContext {
                            operation: self.context.operation,
                            request_body: request_body.as_deref(),
                            response: ResponseView {
                                status,
                                headers,
                                body,
                            },
                        })
                        .ok()
                        .flatten()
                } else {
                    None
                }
            }
        };
        // What the upstream did not report is estimated locally, for answers
        // that were served: a rejected call consumed nothing to meter.
        if self.funnel.policy().usage
            && status.is_some_and(|s| s.is_success())
            && let Some(estimator) = self.context.attempt.request.snapshot.estimation.as_ref()
        {
            let request_body = self.request_body.lock().unwrap().clone();
            let attempt = &self.context.attempt;
            usage = estimator.complete(
                self.context.operation,
                &attempt.credential.provider_id,
                attempt.request.target.upstream_model.as_deref(),
                request_body.as_deref(),
                self.response_bytes.load(Ordering::Relaxed),
                usage,
            );
        }
        if end == UsageStreamEnd::Interrupted
            && let Some(usage) = usage.as_mut()
        {
            usage.completeness = gproxy_channel::channel::UsageCompleteness::Partial;
        }
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
        let (tx, rx) = tokio::sync::oneshot::channel();
        crate::rt::spawn(async move {
            if let Some(sink) = sink {
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
        let _ = rx.await;
    }

    /// End of this exchange: finish usage (stream observer or extractor over
    /// the accumulated body), close capture, and settle the request if this
    /// exchange was terminal. Idempotent.
    pub async fn finish(
        &self,
        end: UsageStreamEnd,
        status: Option<http::StatusCode>,
        headers: Option<&http::HeaderMap>,
        accumulated: Option<&[u8]>,
        now_ms: i64,
    ) {
        if let Some(sink) = self.settle_sync(end, status, headers, accumulated, now_ms) {
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
        headers: http::HeaderMap,
        accumulated: Option<Vec<u8>>,
        now_ms: i64,
    ) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }
        self.dropped.store(true, Ordering::SeqCst);
        let Some(sink) = self.settle_sync(
            end,
            Some(status),
            Some(&headers),
            accumulated.as_deref(),
            now_ms,
        ) else {
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

    fn start_usage_observer(&self, response: &WireResponse<HttpBody>) {
        if !self.funnel.policy().usage {
            return;
        }
        let Some(framing) = response_framing(self.context.operation, &response.headers) else {
            return;
        };
        let Some(stream) = self.channel.usage_stream() else {
            return;
        };
        let request_body = self.request_body.lock().unwrap().clone();
        let observer = stream.start(UsageStreamContext {
            operation: self.context.operation,
            request_body: request_body.as_deref(),
            status: response.status,
            headers: &response.headers,
            transport: UsageTransport::Http {
                framing: Some(framing),
            },
        });
        if let Ok(observer) = observer {
            *self.usage_observer.lock().unwrap() = Some(observer);
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
                    *exchange.request_body.lock().unwrap() = Some(bytes.clone());
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
                    exchange
                        .finish(
                            UsageStreamEnd::Interrupted,
                            None,
                            None,
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
            exchange.start_usage_observer(&response);
            let framing = response_framing(exchange.context.operation, &response.headers);
            let status = response.status;
            let headers = response.headers.clone();
            let body = observed_body(exchange, response.body, framing, status, headers.clone());
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
                    self.exchange
                        .finish(
                            UsageStreamEnd::Interrupted,
                            None,
                            None,
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
                    let body = observed_body(
                        self.exchange.clone(),
                        response.body,
                        None,
                        status,
                        headers.clone(),
                    );
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

impl Drop for Exchange {
    fn drop(&mut self) {
        // Covers a dropped send/handshake future before any response body guard.
        if let Some(sink) = self.settle_sync(
            UsageStreamEnd::Interrupted,
            None,
            None,
            None,
            crate::api::lifecycle::now_ms(),
        ) {
            let closed = self.closed.lock().unwrap().take();
            if sink.is_some() || closed.is_some() {
                crate::rt::spawn(async move {
                    if let Some(sink) = sink {
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
