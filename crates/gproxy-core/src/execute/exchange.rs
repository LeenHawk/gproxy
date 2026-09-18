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
    atomic::{AtomicBool, AtomicU64, Ordering},
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
        let policy = funnel.policy().capture;
        let capture = match policy {
            CapturePolicy::Off => None,
            policy => Some(funnel.observer().capture(&context, policy)),
        };
        funnel.trace(TraceEvent::ExchangeStarted(&context));
        Arc::new(Self {
            context,
            funnel,
            channel,
            limits,
            response_rules,
            capture: Mutex::new(capture),
            sequence: AtomicU64::new(0),
            request_body: Mutex::new(None),
            usage_observer: Mutex::new(None),
            terminal: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        })
    }

    /// Mark that the caller receives this exchange's body: its end settles the request.
    pub fn make_terminal(&self) {
        self.terminal.store(true, Ordering::SeqCst);
    }

    pub fn wants_full_capture(&self) -> bool {
        self.funnel.policy().capture == CapturePolicy::Full
    }

    pub fn record(&self, event: CaptureEvent<'_>) {
        if let Some(sink) = self.capture.lock().unwrap().as_mut() {
            let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
            sink.record(sequence, event);
        }
    }

    pub fn observe_chunk(&self, chunk: &[u8]) {
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
            provider_id: attempt.credential.provider_id.clone(),
            credential_id: attempt.credential.id.clone(),
            upstream_model: attempt.request.target.upstream_model.clone(),
            usage,
        });
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
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        let observer = self.usage_observer.lock().unwrap().take();
        match observer {
            Some(observer) => {
                if let Ok(usage) = observer.finish(end) {
                    self.report_usage(usage);
                }
            }
            None => {
                if let (Some(body), Some(status), Some(headers), Some(extractor)) =
                    (accumulated, status, headers, self.channel.usage_extractor())
                    && self.funnel.policy().usage
                {
                    let request_body = self.request_body.lock().unwrap().clone();
                    let usage = extractor.extract(UsageContext {
                        operation: self.context.operation,
                        request_body: request_body.as_deref(),
                        response: ResponseView {
                            status,
                            headers,
                            body,
                        },
                    });
                    if let Ok(usage) = usage {
                        self.report_usage(usage);
                    }
                }
            }
        }
        let sink = self.capture.lock().unwrap().take();
        if let Some(sink) = sink {
            sink.finish(match end {
                UsageStreamEnd::Complete => crate::CaptureEnd::Complete,
                UsageStreamEnd::Interrupted => crate::CaptureEnd::Interrupted,
            })
            .await;
        }
        self.funnel.trace(TraceEvent::ExchangeFinished {
            exchange: &self.context,
            status,
            finished_at_ms: now_ms,
        });
        if self.terminal.load(Ordering::SeqCst) {
            let state = match end {
                UsageStreamEnd::Complete => UsageState::Completed,
                UsageStreamEnd::Interrupted => {
                    if self.context.attempt.request.cancellation.is_cancelled() {
                        UsageState::Cancelled
                    } else {
                        UsageState::Failed
                    }
                }
            };
            self.funnel.finish(state).await;
        }
    }

    /// Drop path: cannot await, so hand the work to the runtime.
    pub fn finish_detached(self: Arc<Self>, end: UsageStreamEnd, now_ms: i64) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                self.finish(end, None, None, None, now_ms).await;
            });
        } else if self.terminal.load(Ordering::SeqCst) {
            // No runtime to run the funnel on; at least resolve the completion.
            self.finished.store(true, Ordering::SeqCst);
            self.funnel.clone().finish_detached(UsageState::Cancelled);
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
pub(crate) struct ObservedClient<'a> {
    inner: &'a dyn OutboundClient,
    exchange: Arc<Exchange>,
}

impl<'a> ObservedClient<'a> {
    pub fn new(inner: &'a dyn OutboundClient, exchange: Arc<Exchange>) -> Self {
        Self { inner, exchange }
    }
}

impl OutboundClient for ObservedClient<'_> {
    fn send<'b>(
        &'b self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'b, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let exchange = self.exchange.clone();
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
            let response = self.inner.send(request).await?;
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
            let connection = self.inner.connect(request).await?;
            match &connection {
                UpstreamConnection::Connected { handshake, .. } => {
                    self.exchange.record(CaptureEvent::ResponseHead {
                        status: handshake.status,
                        headers: &handshake.headers,
                    });
                }
                UpstreamConnection::Rejected(response) => {
                    self.exchange.record(CaptureEvent::ResponseHead {
                        status: response.status,
                        headers: &response.headers,
                    });
                }
            }
            // Frame observation and rejected-body observation arrive with the
            // WebSocket execution path.
            Ok(connection)
        })
    }
}
