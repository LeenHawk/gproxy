//! Usage of a native call, read from the response the channel returned.
//!
//! A channel shapes whatever its upstream answered into the standard
//! response of the operation it served, so usage is read there, once, by
//! operation and dialect (`gproxy_protocol::usage`), with the channel's
//! [`UsageExtras`] adding what only its vendor reports. The upstream's raw
//! bytes are still observed by each physical exchange for capture and for
//! the response reason; they are no longer read for usage, because before
//! shaping they are in whatever form the vendor chose.
//!
//! One native call is one metered exchange, however many physical sends the
//! channel made to serve it: a bootstrap, a token exchange or a quota probe
//! is captured but never metered. The reading is attributed to the physical
//! exchange that answered (see [`UsageGate`] for how it is told apart) and
//! reaches that exchange's capture through the gate, because the capture
//! may close before the shaped response has been read to its end.
//!
//! [`UsageExtras`]: gproxy_channel::channel::UsageExtras

use super::Funnel;
use crate::{AttemptContext, ExchangeUsage, convert::response_framing};
use futures_util::Stream;
use gproxy_channel::{
    BaseChannel,
    channel::{
        NormalizedUsage, UsageCompleteness, UsageSource, UsageStreamEnd, UsageTransport,
        with_extras,
    },
};
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    connection::{ByteStream, Bytes, TransportError},
    usage::{self, UsageReader},
};
use serde_json::Value;
use std::{
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
};
use tokio::sync::watch;

// ------------------------------------------------------------------ reading

/// What a response is read as.
enum Source {
    /// A stream, watched event by event.
    Stream(Box<UsageReader>),
    /// A complete body, held until its end. `None` once it outgrew the cap,
    /// which leaves nothing to read.
    Whole(Option<Vec<u8>>),
    /// A response of an operation that reports no usage.
    Nothing,
}

/// Reads one standard response for its usage, as it passes.
pub(crate) struct Reading {
    channel: Arc<dyn BaseChannel>,
    operation: OperationKey,
    headers: http::HeaderMap,
    source: Source,
    /// Bytes seen, for the local output estimate.
    bytes: u64,
    /// The most a whole body is held for.
    cap: u64,
    first_output_at: Option<web_time::Instant>,
}

impl Reading {
    /// An HTTP response of `operation`. A response that declares a stream
    /// framing is watched, and so is a streamed operation's response that
    /// does not say it is plain JSON; anything else — a buffered reply to a
    /// streamed request included — is read whole once it has ended, and is
    /// held only when there is something in it to read.
    pub(crate) fn http(
        channel: Arc<dyn BaseChannel>,
        operation: OperationKey,
        headers: &http::HeaderMap,
        cap: u64,
    ) -> Self {
        let extras = channel.usage_extras().is_some();
        let framing = response_framing(operation, headers);
        let json = headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim_start().starts_with("application/json"));
        let streamed =
            framing.is_some() || (operation.operation == Operation::StreamGenerateContent && !json);
        let source = if streamed {
            UsageReader::new(
                operation.operation,
                operation.dialect,
                UsageTransport::Http { framing },
            )
            .map_or(Source::Nothing, |mut reader| {
                if extras {
                    reader.keep_usage_event();
                }
                Source::Stream(Box::new(reader))
            })
        } else if usage::reads(operation.operation, operation.dialect) || extras {
            Source::Whole(Some(Vec::new()))
        } else {
            Source::Nothing
        };
        Self {
            channel,
            operation,
            headers: headers.clone(),
            source,
            bytes: 0,
            cap,
            first_output_at: None,
        }
    }

    /// A websocket session of `operation`, one text message per event.
    pub(crate) fn websocket(channel: Arc<dyn BaseChannel>, operation: OperationKey) -> Self {
        let source = UsageReader::new(
            operation.operation,
            operation.dialect,
            UsageTransport::WebSocket,
        )
        .map_or(Source::Nothing, |mut reader| {
            if channel.usage_extras().is_some() {
                reader.keep_usage_event();
            }
            Source::Stream(Box::new(reader))
        });
        Self {
            channel,
            operation,
            headers: http::HeaderMap::new(),
            source,
            bytes: 0,
            cap: 0,
            first_output_at: None,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) {
        self.bytes = self.bytes.saturating_add(chunk.len() as u64);
        match &mut self.source {
            Source::Stream(reader) => {
                reader.push(chunk);
                if self.first_output_at.is_none() && reader.output_started() {
                    self.first_output_at = Some(web_time::Instant::now());
                }
            }
            Source::Whole(slot) => {
                if let Some(body) = slot {
                    if (body.len() + chunk.len()) as u64 > self.cap {
                        *slot = None;
                    } else {
                        body.extend_from_slice(chunk);
                    }
                }
            }
            Source::Nothing => {}
        }
    }

    pub(crate) fn push_message(&mut self, text: &str) {
        if let Source::Stream(reader) = &mut self.source {
            reader.push_message(text);
        }
    }

    /// The reading: the standard one, then the channel's extras over the
    /// JSON that carried it. A cut stream or body is never complete.
    pub(crate) fn finish(self, end: UsageStreamEnd) -> Option<NormalizedUsage> {
        let (standard, root) = match self.source {
            Source::Stream(reader) => {
                let root = reader.usage_event();
                (reader.finish(end), root)
            }
            Source::Whole(Some(body)) => {
                let standard =
                    usage::whole(self.operation.operation, self.operation.dialect, &body);
                let root = self
                    .channel
                    .usage_extras()
                    .and_then(|_| serde_json::from_slice::<Value>(&body).ok());
                (standard, root)
            }
            Source::Whole(None) | Source::Nothing => (None, None),
        };
        let mut reading = with_extras(
            self.channel.usage_extras(),
            UsageSource {
                operation: self.operation,
                headers: &self.headers,
                root: root.as_ref().unwrap_or(&Value::Null),
            },
            standard,
        );
        if end == UsageStreamEnd::Interrupted
            && let Some(usage) = reading.as_mut()
        {
            usage.completeness = UsageCompleteness::Partial;
        }
        reading
    }
}

// --------------------------------------------------------------------- gate

#[derive(Clone)]
enum Gate {
    /// The channel has not returned its response yet: every send it starts
    /// may be the one that answers.
    Open,
    /// The channel returned its response; later sends are not candidates.
    Returned,
    /// The reading is final.
    Done {
        chosen: Option<String>,
        usage: Option<Box<NormalizedUsage>>,
    },
}

/// A physical send that may have answered the call.
struct Candidate {
    capture_id: String,
    /// Response bytes it received so far.
    received: Arc<AtomicU64>,
}

/// Hands a native call's reading to the capture of the physical exchange it
/// is attributed to, and tells every other exchange of the call that it has
/// none.
///
/// The exchange that answered is the one of the sends started before the
/// channel returned its response that received the most bytes, the latest
/// on a tie. A bootstrap, a token exchange or an upload before the answer
/// receives little; a clean-up after it is started too late to count.
/// Choosing by what was received, rather than by order alone, keeps a
/// channel that collects the answer and then deletes a conversation from
/// having its usage filed under the deletion. The choice only decides which
/// capture shows the reading: a native call is metered once either way.
pub(crate) struct UsageGate {
    state: watch::Sender<Gate>,
    candidates: Mutex<Vec<Candidate>>,
}

impl UsageGate {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: watch::Sender::new(Gate::Open),
            candidates: Mutex::new(Vec::new()),
        })
    }

    /// A physical send of this call started; `received` counts the bytes of
    /// its response.
    pub(crate) fn started(&self, capture_id: &str, received: Arc<AtomicU64>) {
        if matches!(*self.state.borrow(), Gate::Open) {
            self.candidates.lock().unwrap().push(Candidate {
                capture_id: capture_id.to_owned(),
                received,
            });
        }
    }

    /// The channel returned its response.
    fn returned(&self) {
        self.state.send_if_modified(|state| {
            let open = matches!(state, Gate::Open);
            if open {
                *state = Gate::Returned;
            }
            open
        });
    }

    fn is_candidate(&self, capture_id: &str) -> bool {
        self.candidates
            .lock()
            .unwrap()
            .iter()
            .any(|candidate| candidate.capture_id == capture_id)
    }

    /// The capture the reading belongs to, `None` when the channel made no
    /// send.
    fn chosen(&self) -> Option<String> {
        self.candidates
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .max_by_key(|(index, candidate)| (candidate.received.load(Ordering::Relaxed), *index))
            .map(|(_, candidate)| candidate.capture_id.clone())
    }

    /// Make the reading final. Only the first call counts.
    fn settle(&self, chosen: Option<String>, usage: Option<NormalizedUsage>) {
        self.state.send_if_modified(|state| {
            if matches!(state, Gate::Done { .. }) {
                return false;
            }
            *state = Gate::Done {
                chosen,
                usage: usage.map(Box::new),
            };
            true
        });
    }

    /// The reading for the capture `capture_id`, once it is known whether
    /// there is one. `None` for every capture but the chosen one; a send
    /// started after the response returned learns that at once.
    pub(crate) async fn wait(&self, capture_id: &str) -> Option<NormalizedUsage> {
        let mut receiver = self.state.subscribe();
        let state = receiver
            .wait_for(|state| match state {
                Gate::Open => false,
                Gate::Returned => !self.is_candidate(capture_id),
                Gate::Done { .. } => true,
            })
            .await
            .ok()?
            .clone();
        match state {
            Gate::Done {
                chosen: Some(id),
                usage,
            } if id == capture_id => usage.map(|usage| *usage),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ metered

/// One native call's metering, from the moment it starts until its reading
/// is final. Dropped unfinished, it settles the gate with no reading, so no
/// capture waits on it forever.
pub(crate) struct Meter {
    funnel: Arc<Funnel>,
    attempt: Arc<AttemptContext>,
    gate: Arc<UsageGate>,
    /// Holds the request's settlement until the reading is recorded: the
    /// shaped response may be let go of after the request's own body ended.
    settlement: Option<tokio::sync::oneshot::Sender<()>>,
    /// The standard request, which is the operation's own shape, for the
    /// local input estimate.
    request_body: Option<Bytes>,
    status: http::StatusCode,
    reading: Option<Reading>,
    started_at: web_time::Instant,
}

impl Meter {
    pub(crate) fn new(
        funnel: Arc<Funnel>,
        attempt: Arc<AttemptContext>,
        gate: Arc<UsageGate>,
        request_body: Option<Bytes>,
    ) -> Self {
        Self {
            settlement: Some(funnel.hold_settlement()),
            funnel,
            attempt,
            gate,
            request_body,
            status: http::StatusCode::OK,
            reading: None,
            started_at: web_time::Instant::now(),
        }
    }

    /// Meter the response the channel returned, and hand it back. A body
    /// already whole is read here; a stream is read as the caller reads it,
    /// and settles when it ends, fails or is dropped.
    pub(crate) fn response(
        mut self,
        channel: Arc<dyn BaseChannel>,
        operation: OperationKey,
        cap: u64,
        response: WireResponse<HttpBody>,
    ) -> WireResponse<HttpBody> {
        self.gate.returned();
        self.status = response.status;
        self.reading = Some(Reading::http(channel, operation, &response.headers, cap));
        let WireResponse {
            status,
            headers,
            body,
        } = response;
        let body = match body {
            HttpBody::Bytes(bytes) => {
                self.push(&bytes);
                self.finish(UsageStreamEnd::Complete);
                HttpBody::Bytes(bytes)
            }
            HttpBody::Stream(inner) => HttpBody::Stream(Box::pin(Tap {
                inner,
                meter: Some(self),
            })),
        };
        WireResponse {
            status,
            headers,
            body,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if let Some(reading) = self.reading.as_mut() {
            reading.push(chunk);
        }
    }

    /// Final reading, local estimate and report, synchronously: a dropped
    /// stream must be metered before anything can settle the request.
    fn finish(&mut self, end: UsageStreamEnd) {
        let Some(reading) = self.reading.take() else {
            self.gate.settle(self.gate.chosen(), None);
            self.settlement = None;
            return;
        };
        let duration_ms = i64::try_from(self.started_at.elapsed().as_millis()).ok();
        let ttft_ms = reading.first_output_at.and_then(|first| {
            i64::try_from(first.duration_since(self.started_at).as_millis()).ok()
        });
        let bytes = reading.bytes;
        let operation = reading.operation;
        let mut usage = reading.finish(end);
        // What the upstream did not report is estimated locally, for answers
        // that were served: a rejected call consumed nothing to meter.
        let attempt = &self.attempt;
        if self.status.is_success()
            && let Some(estimator) = attempt.request.snapshot.estimation.as_ref()
        {
            usage = estimator.complete(
                operation,
                &attempt.credential.provider_id,
                attempt.request.target.upstream_model.as_deref(),
                self.request_body.as_deref(),
                bytes,
                usage,
            );
        }
        // No physical send, no upstream consumption: a channel that answered
        // locally is not metered. The reading is recorded before the gate
        // and the settlement hold are released, so no summary misses it.
        let chosen = self.gate.chosen();
        if let (Some(capture_id), Some(usage)) = (chosen.clone(), usage.as_ref()) {
            self.funnel.record_exchange_usage(ExchangeUsage {
                duration_ms,
                ttft_ms,
                capture_id,
                attempt_id: attempt.attempt_id.clone(),
                attempt_ordinal: attempt.ordinal,
                provider_id: attempt.credential.provider_id.clone(),
                credential_id: attempt.credential.id.clone(),
                upstream_model: attempt.request.target.upstream_model.clone(),
                usage: usage.clone(),
                cost: None,
            });
        }
        self.gate.settle(chosen, usage);
        self.settlement = None;
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        if self.reading.is_some() {
            self.finish(UsageStreamEnd::Interrupted);
        } else {
            // Idempotent: a finished meter already settled the gate.
            self.gate.settle(self.gate.chosen(), None);
        }
    }
}

/// The shaped body on its way to the caller, read as it passes. Ending,
/// failing or being dropped settles the meter, exactly once.
struct Tap {
    inner: ByteStream,
    meter: Option<Meter>,
}

impl Stream for Tap {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let item = std::task::ready!(self.inner.as_mut().poll_next(cx));
        match &item {
            Some(Ok(chunk)) => {
                if let Some(meter) = self.meter.as_mut() {
                    meter.push(chunk);
                }
            }
            Some(Err(_)) => {
                if let Some(mut meter) = self.meter.take() {
                    meter.finish(UsageStreamEnd::Interrupted);
                }
            }
            None => {
                if let Some(mut meter) = self.meter.take() {
                    meter.finish(UsageStreamEnd::Complete);
                }
            }
        }
        Poll::Ready(item)
    }
}
