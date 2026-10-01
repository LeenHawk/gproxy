//! Observation extension point: capture, usage and tracing. StoreObserver is
//! the default implementation; hosts can replace it explicitly. The policy is
//! queried before work starts. Core owns pricing/settlement; the observer owns
//! persistence and redaction. Nothing here is a public pipeline hook.

use crate::{AttemptContext, ExchangeContext, RequestContext, UsageReport};
use gproxy_channel::ChannelError;
use gproxy_protocol::{capability::CapabilityFuture, connection::WsFrame};
use http::{HeaderMap, Method, StatusCode, Uri};
use std::time::Duration;

/// Answered once per request before any attempt. Disabled work is never
/// performed and then discarded: no usage extraction, no body observation and
/// no event formatting happens for a switched-off item. Deliberately without a
/// Default so a host cannot silently opt out of settlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationPolicy {
    /// Observe upstream usage and deliver exactly one `UsageReport`.
    pub usage: bool,
    pub capture: CapturePolicy,
    /// Deliver attempt/exchange lifecycle events.
    pub trace: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturePolicy {
    Off,
    /// Heads and response reason only: no request/response bytes or WS payloads
    /// are delivered to the capture sink. Channels may inspect response bytes
    /// to classify the reason without retaining the body in the log.
    Metadata,
    /// Heads plus every body chunk and WS frame in observed order.
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureDirection {
    /// Gateway -> provider.
    Request,
    /// Provider -> gateway, including WS control messages.
    Response,
}

/// One observed unit of a physical upstream exchange. Bytes are borrowed for
/// the duration of the call; chunks are transport chunks, not decoded events.
pub enum CaptureEvent<'a> {
    /// A generation carried by an existing physical WebSocket connection.
    TurnStart {
        connection_id: &'a str,
        lane: Option<&'a str>,
    },
    RequestHead {
        method: &'a Method,
        uri: &'a Uri,
        headers: &'a HeaderMap,
    },
    RequestChunk(&'a [u8]),
    ResponseHead {
        status: StatusCode,
        headers: &'a HeaderMap,
    },
    ResponseChunk(&'a [u8]),
    /// One complete frame after a successful WS handshake.
    Frame {
        direction: CaptureDirection,
        frame: &'a WsFrame,
        turn_id: Option<&'a str>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureEnd {
    Complete,
    /// Transport error, cancellation or deadline before the exchange finished.
    Interrupted,
    /// The caller dropped the body/socket or explicitly cancelled.
    Cancelled,
}

/// Per-exchange capture state opened by the Observer. `sequence` is monotonic
/// across both directions of the exchange. The sink copies only what its
/// policy retains and never rewrites, reorders or delays the delivered stream.
/// Recording failures are the host's concern and must not fail the request.
pub trait CaptureSink: Send {
    fn record(&mut self, sequence: u64, event: CaptureEvent<'_>);
    /// Persist the parent before a WS turn can reference its capture ID.
    fn flush(&self) -> CapabilityFuture<'static, ()> {
        Box::pin(async {})
    }
    /// Native usage for this physical exchange, independent of body logging.
    fn usage(&mut self, _usage: &gproxy_channel::channel::NormalizedUsage) {}
    /// Channel classification; independent of whether usage/body capture is enabled.
    fn reason(&mut self, _reason: gproxy_channel::channel::ResponseReason) {}
    /// Flush and close. Awaited off the response path.
    fn finish(self: Box<Self>, end: CaptureEnd) -> CapabilityFuture<'static, ()>;
}

/// Reported only after the attempt actually completed. Response headers or a
/// WS upgrade alone do not make an attempt successful.
#[derive(Debug)]
pub enum AttemptOutcome {
    Succeeded {
        status: StatusCode,
    },
    Rejected {
        status: StatusCode,
        retry_after: Option<Duration>,
    },
    Failed(ChannelError),
    Cancelled,
    DeadlineExceeded,
}

/// Borrowed lifecycle facts. The host formats or serializes only when it wants
/// them; core never pre-renders strings for a disabled trace policy.
pub enum TraceEvent<'a> {
    AttemptStarted(&'a AttemptContext),
    AttemptFinished {
        attempt: &'a AttemptContext,
        outcome: &'a AttemptOutcome,
        finished_at_ms: i64,
    },
    ExchangeStarted(&'a ExchangeContext),
    ExchangeFinished {
        exchange: &'a ExchangeContext,
        status: Option<StatusCode>,
        finished_at_ms: i64,
    },
    /// A refresh persisted and published this version during the attempt.
    CredentialRefreshed {
        attempt: &'a AttemptContext,
        version: i64,
    },
    /// A caller budget was already spent before the first attempt: the
    /// request was rejected without touching any credential.
    BudgetRejected {
        request: &'a RequestContext,
        quota_id: &'a str,
        window_key: &'a str,
        resets_at_ms: Option<i64>,
    },
}

/// The host side of the funnel. Core consults `policy` first, then reports
/// through the enabled methods only. Downstream (client <-> gateway) capture and
/// linking to these upstream exchanges stay with the host, correlated by
/// `request_id`, `attempt_id` and `capture_id`.
pub trait Observer: Send + Sync {
    /// Cheap, synchronous and side-effect free. Decide per opaque scope,
    /// provider and operation; core does not cache the answer across requests.
    fn policy(&self, request: &RequestContext) -> ObservationPolicy;

    /// Open capture for one physical exchange. Called only when the request's
    /// policy is not `CapturePolicy::Off`; `policy` is that non-Off value.
    fn capture(&self, exchange: &ExchangeContext, policy: CapturePolicy) -> Box<dyn CaptureSink>;

    /// Settlement funnel: exactly once per request whose policy enabled usage,
    /// including cancelled and failed requests, after the response stream or
    /// socket finished. The same report resolves the caller's `UsageCompletion`;
    /// dropping that future does not skip this call.
    fn usage<'a>(
        &'a self,
        request: &'a RequestContext,
        report: &'a UsageReport,
    ) -> CapabilityFuture<'a, ()>;

    /// Called only when the request's policy enabled tracing.
    fn trace(&self, event: TraceEvent<'_>);
}

/// Snapshot-pinned switches for the built-in Store observer.
#[derive(Clone, Copy, Debug)]
pub struct ObservationSettings {
    pub settlement: bool,
    pub usage: bool,
    pub upstream_log: bool,
    pub upstream_log_body: bool,
    pub redact: bool,
    pub trace: bool,
}
impl Default for ObservationSettings {
    fn default() -> Self {
        Self {
            settlement: true,
            usage: true,
            upstream_log: false,
            upstream_log_body: false,
            redact: true,
            trace: true,
        }
    }
}
impl ObservationSettings {
    pub(crate) fn from_setting(
        setting: Option<&gproxy_store::entity::config::setting::Model>,
    ) -> Self {
        setting
            .map(|s| Self {
                settlement: s.enable_settlement,
                usage: s.enable_usage,
                upstream_log: s.enable_upstream_log,
                upstream_log_body: s.enable_upstream_log_body,
                redact: !s.disable_log_redaction,
                trace: s.enable_tracing,
            })
            .unwrap_or_default()
    }
}
