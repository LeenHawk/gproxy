//! Store-backed upstream capture and per-request usage. One writer per exchange
//! preserves event order, flushes on finish, and closes interrupted on sender drop.
use crate::{
    CaptureEnd, CaptureEvent, CapturePolicy, CaptureSink, ExchangeContext, ObservationPolicy,
    Observer, RequestContext, TraceEvent, UsageReport, api::lifecycle::now_ms,
};
use gproxy_channel::channel::NormalizedUsage;
use gproxy_protocol::{capability::CapabilityFuture, connection::WsFrame};
use gproxy_seaorm::{BatchConnectionTrait, FixedDecimal};
use gproxy_store::{
    Store,
    entity::usage::{capture_event as event, capture_record as record, usage_record},
};
use sea_orm::{EntityTrait, QueryTrait, Set};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot};

pub struct StoreObserver<C> {
    store: Arc<Store<C>>,
}
impl<C> StoreObserver<C> {
    pub fn new(store: Arc<Store<C>>) -> Self {
        Self { store }
    }
}
impl<C: BatchConnectionTrait + Send + Sync + 'static> Observer for StoreObserver<C> {
    fn policy(&self, request: &RequestContext) -> ObservationPolicy {
        let s = request.snapshot.observation;
        ObservationPolicy {
            usage: s.settlement || s.usage,
            capture: if !s.upstream_log {
                CapturePolicy::Off
            } else if s.upstream_log_body {
                CapturePolicy::Full
            } else {
                CapturePolicy::Metadata
            },
            trace: s.trace,
        }
    }
    fn capture(&self, exchange: &ExchangeContext, policy: CapturePolicy) -> Box<dyn CaptureSink> {
        let attempt = &exchange.attempt;
        let request = &attempt.request;
        let full = policy == CapturePolicy::Full;
        let row = record::ActiveModel {
            id: Set(exchange.capture_id.clone()),
            initiator_request_id: Set(Some(request.request_id.clone())),
            attempt_id: Set(Some(attempt.attempt_id.clone())),
            attempt_ordinal: Set(Some(attempt.ordinal as i32)),
            side: Set(record::CaptureSide::Upstream),
            kind: Set(record::CaptureKind::Http),
            user_id: Set(request.attribution.user_id.clone()),
            api_key_id: Set(request.attribution.api_key_id.clone()),
            provider_id: Set(Some(attempt.credential.provider_id.clone())),
            credential_id: Set(Some(attempt.credential.id.clone())),
            agent_assignment_id: Set(attempt
                .agent_assignment
                .as_ref()
                .map(|a| a.assignment_id.clone())),
            model: Set(request.target.upstream_model.clone()),
            operation: Set(Some(exchange.operation.operation.id().into())),
            request_framing: Set(record::BodyFraming::Bytes),
            response_framing: Set(record::BodyFraming::Bytes),
            request_body_state: Set(if full {
                record::CaptureBodyState::Recording
            } else {
                record::CaptureBodyState::NotCaptured
            }),
            response_body_state: Set(if full {
                record::CaptureBodyState::Recording
            } else {
                record::CaptureBodyState::NotCaptured
            }),
            started_at_ms: Set(exchange.started_at_ms),
            ..Default::default()
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let lost = Arc::new(AtomicBool::new(false));
        let store = self.store.clone();
        let worker_lost = lost.clone();
        let cancellation = request.cancellation.clone();
        crate::rt::spawn(async move {
            write_capture(store, row, rx, worker_lost, full, cancellation).await;
        });
        Box::new(StoreCapture {
            id: exchange.capture_id.clone(),
            tx,
            lost,
            full,
            redact: request.snapshot.observation.redact,
            secrets: secret_values(&attempt.credential_version.secret),
        })
    }
    fn usage<'a>(
        &'a self,
        request: &'a RequestContext,
        report: &'a UsageReport,
    ) -> CapabilityFuture<'a, ()> {
        Box::pin(async move {
            if !request.snapshot.observation.usage {
                return;
            }
            let aggregate = if report.exchanges.is_empty() {
                NormalizedUsage::default()
            } else {
                NormalizedUsage::aggregate(report.exchanges.iter().map(|e| &e.usage))
            };
            let mut metrics = usage_json(report.downstream_usage.as_ref().unwrap_or(&aggregate));
            metrics["state"] = json!(format!("{:?}", report.state).to_lowercase());
            metrics["exchanges"] = json!(report.exchanges.iter().map(|e| json!({
                "capture_id": e.capture_id, "attempt_id": e.attempt_id, "attempt_ordinal": e.attempt_ordinal, "provider_id": e.provider_id,
                "credential_id": e.credential_id, "model": e.upstream_model,
                "usage": usage_json(&e.usage),
                "cost": e.cost.as_ref().map(|c| json!({"amount": c.amount.to_string(), "currency": c.currency})),
            })).collect::<Vec<_>>());
            metrics["cost"] = json!(
                report
                    .cost
                    .as_ref()
                    .map(|c| json!({"amount": c.amount.to_string(), "currency": c.currency}))
            );
            let row = usage_record::ActiveModel {
                request_id: Set(report.request_id.clone()),
                user_id: Set(request.attribution.user_id.clone()),
                api_key_id: Set(request.attribution.api_key_id.clone()),
                subscription_id: Set(request.attribution.subscription_id.clone()),
                model: Set(request
                    .attribution
                    .model
                    .clone()
                    .or_else(|| request.target.upstream_model.clone())
                    .unwrap_or_default()),
                operation: Set(request.operation.operation.id().into()),
                metrics: Set(metrics),
                cost: Set(report
                    .cost
                    .as_ref()
                    .and_then(|c| FixedDecimal::rounded(c.amount).ok())),
                started_at_ms: Set(request.started_at_ms),
                ended_at_ms: Set(Some(now_ms())),
            };
            if let Err(error) = self.store.usage_records().create_many(vec![row]).await {
                tracing::error!(request_id = %report.request_id, %error, "usage persistence failed");
            }
        })
    }
    fn trace(&self, event: TraceEvent<'_>) {
        if let TraceEvent::AttemptFinished {
            attempt, outcome, ..
        } = event
        {
            tracing::debug!(request_id = %attempt.request.request_id, attempt_id = %attempt.attempt_id, ?outcome, "upstream attempt finished");
        }
    }
}

enum Write {
    Head(Box<record::ActiveModel>),
    Event(event::ActiveModel),
    Finish(CaptureEnd, oneshot::Sender<()>),
}
struct StoreCapture {
    id: String,
    tx: mpsc::UnboundedSender<Write>,
    lost: Arc<AtomicBool>,
    full: bool,
    redact: bool,
    secrets: Vec<Vec<u8>>,
}
impl StoreCapture {
    fn send(&self, value: Write) {
        if self.tx.send(value).is_err() {
            self.lost.store(true, Ordering::Relaxed);
        }
    }
    fn bytes(
        &self,
        sequence: u64,
        direction: event::CaptureDirection,
        kind: event::CaptureEventKind,
        payload: &[u8],
    ) {
        if !self.full {
            return;
        }
        let payload = if self.redact {
            redact_bytes(payload, &self.secrets)
        } else {
            payload.to_vec()
        };
        self.send(Write::Event(event::ActiveModel {
            capture_id: Set(self.id.clone()),
            sequence: Set(sequence as i64),
            direction: Set(direction),
            kind: Set(kind),
            payload: Set(payload),
            observed_at_ms: Set(now_ms()),
            ..Default::default()
        }));
    }
}
impl CaptureSink for StoreCapture {
    fn record(&mut self, sequence: u64, event: CaptureEvent<'_>) {
        use event::{CaptureDirection as D, CaptureEventKind as K};
        let mut patch = record::ActiveModel {
            id: Set(self.id.clone()),
            ..Default::default()
        };
        match event {
            CaptureEvent::RequestHead {
                method,
                uri,
                headers,
            } => {
                patch.request_method = Set(Some(method.to_string()));
                let url = uri.to_string();
                let url = url.split('?').next().unwrap_or(&url);
                patch.request_url = Set(Some(if self.redact {
                    String::from_utf8_lossy(&redact_bytes(url.as_bytes(), &self.secrets))
                        .into_owned()
                } else {
                    url.into()
                }));
                patch.request_query = Set(uri.query().map(|q| {
                    if self.redact {
                        redact_query(q)
                    } else {
                        q.into()
                    }
                }));
                patch.request_headers = Set(Some(headers_json(headers, self.redact)));
            }
            CaptureEvent::ResponseHead { status, headers } => {
                patch.response_status = Set(Some(i32::from(status.as_u16())));
                patch.response_headers = Set(Some(headers_json(headers, self.redact)));
                patch.first_response_at_ms = Set(Some(now_ms()));
                if status == http::StatusCode::SWITCHING_PROTOCOLS {
                    patch.kind = Set(record::CaptureKind::WsConnection);
                    patch.request_framing = Set(record::BodyFraming::WebSocket);
                    patch.response_framing = Set(record::BodyFraming::WebSocket);
                } else if let Some(ct) = headers
                    .get(http::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                {
                    if ct.starts_with("text/event-stream") {
                        patch.response_framing = Set(record::BodyFraming::Sse);
                    } else if ct.contains("ndjson") {
                        patch.response_framing = Set(record::BodyFraming::NdJson);
                    }
                }
            }
            CaptureEvent::RequestChunk(bytes) => {
                self.bytes(sequence, D::Request, K::Bytes, bytes);
                return;
            }
            CaptureEvent::ResponseChunk(bytes) => {
                self.bytes(sequence, D::Response, K::Bytes, bytes);
                return;
            }
            CaptureEvent::Frame { direction, frame } => {
                let direction = match direction {
                    crate::CaptureDirection::Request => D::Request,
                    crate::CaptureDirection::Response => D::Response,
                };
                match frame {
                    WsFrame::Text(s) => self.bytes(sequence, direction, K::WsText, s.as_bytes()),
                    WsFrame::Binary(b) => self.bytes(sequence, direction, K::WsBinary, b),
                    WsFrame::Ping(b) => self.bytes(sequence, direction, K::WsPing, b),
                    WsFrame::Pong(b) => self.bytes(sequence, direction, K::WsPong, b),
                    WsFrame::Close(close) => self.bytes(
                        sequence,
                        direction,
                        K::WsClose,
                        &serde_json::to_vec(
                            &close
                                .as_ref()
                                .map(|c| json!({"code": c.code, "reason": c.reason})),
                        )
                        .unwrap(),
                    ),
                }
                return;
            }
        }
        self.send(Write::Head(Box::new(patch)));
    }
    fn usage(&mut self, usage: &NormalizedUsage) {
        self.send(Write::Head(Box::new(record::ActiveModel {
            id: Set(self.id.clone()),
            metrics: Set(Some(usage_json(usage))),
            ..Default::default()
        })));
    }
    fn finish(self: Box<Self>, end: CaptureEnd) -> CapabilityFuture<'static, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            if self.tx.send(Write::Finish(end, tx)).is_ok() {
                let _ = rx.await;
            }
        })
    }
}
async fn write_capture<C: BatchConnectionTrait + Send + Sync + 'static>(
    store: Arc<Store<C>>,
    row: record::ActiveModel,
    mut rx: mpsc::UnboundedReceiver<Write>,
    lost: Arc<AtomicBool>,
    full: bool,
    cancellation: tokio_util::sync::CancellationToken,
) {
    let id = row.id.clone().unwrap();
    if let Err(error) = store.capture_records().create_many(vec![row]).await {
        tracing::error!(capture_id = %id, %error, "capture creation failed");
        return;
    }
    let mut ack = None;
    let mut end = CaptureEnd::Interrupted;
    let mut status = None;
    while let Some(write) = rx.recv().await {
        let result = match write {
            Write::Head(patch) => {
                if let sea_orm::ActiveValue::Set(s) = patch.response_status {
                    status = s;
                }
                store
                    .capture_records()
                    .update_many(vec![*patch])
                    .await
                    .map(|_| ())
            }
            Write::Event(event) => {
                let statement =
                    event::Entity::insert(event).build(store.connection().get_database_backend());
                store
                    .connection()
                    .atomic_batch(&[statement])
                    .await
                    .map(|_| ())
                    .map_err(Into::into)
            }
            Write::Finish(value, sender) => {
                end = value;
                ack = Some(sender);
                break;
            }
        };
        if let Err(error) = result {
            lost.store(true, Ordering::Relaxed);
            tracing::error!(capture_id = %id, %error, "capture write failed");
        }
    }
    let incomplete = lost.load(Ordering::Relaxed);
    let state = match end {
        CaptureEnd::Complete if status.is_some_and(|s| s >= 400) => record::CaptureState::Failed,
        CaptureEnd::Complete => record::CaptureState::Completed,
        CaptureEnd::Cancelled => record::CaptureState::Cancelled,
        CaptureEnd::Interrupted if cancellation.is_cancelled() || ack.is_none() => {
            record::CaptureState::Cancelled
        }
        CaptureEnd::Interrupted => record::CaptureState::Failed,
    };
    let body_state = if !full {
        record::CaptureBodyState::NotCaptured
    } else if incomplete || end != CaptureEnd::Complete {
        record::CaptureBodyState::Partial
    } else {
        record::CaptureBodyState::Complete
    };
    let patch = record::ActiveModel {
        id: Set(id.clone()),
        state: Set(state),
        ended_at_ms: Set(Some(now_ms())),
        request_body_state: Set(
            if full && status.is_some() && status != Some(101) && !incomplete {
                record::CaptureBodyState::Complete
            } else {
                body_state
            },
        ),
        response_body_state: Set(body_state),
        error: Set(if incomplete {
            Some("capture writer unavailable or database write failure".into())
        } else {
            match end {
                CaptureEnd::Complete => None,
                CaptureEnd::Interrupted => Some("upstream exchange interrupted".into()),
                CaptureEnd::Cancelled => Some("request cancelled or response dropped".into()),
            }
        }),
        ..Default::default()
    };
    if let Err(error) = store.capture_records().update_many(vec![patch]).await {
        tracing::error!(capture_id = %id, %error, "capture finalization failed");
    }
    if let Some(ack) = ack {
        let _ = ack.send(());
    }
}
fn sensitive(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().replace('-', "_").as_str(),
        "authorization"
            | "proxy_authorization"
            | "cookie"
            | "set_cookie"
            | "x_api_key"
            | "api_key"
            | "key"
            | "access_token"
            | "refresh_token"
            | "token"
            | "password"
            | "secret"
            | "x_goog_api_key"
    )
}
fn headers_json(headers: &http::HeaderMap, redact: bool) -> Value {
    json!(
        headers
            .iter()
            .map(|(k, v)| (
                k.as_str(),
                if redact && sensitive(k.as_str()) {
                    "[redacted]".into()
                } else {
                    String::from_utf8_lossy(v.as_bytes()).into_owned()
                }
            ))
            .collect::<Vec<_>>()
    )
}
fn redact_query(query: &str) -> String {
    query
        .split('&')
        .map(|part| {
            let (key, _) = part.split_once('=').unwrap_or((part, ""));
            let decoded = form_urlencoded::parse(key.as_bytes())
                .next()
                .map(|(k, _)| k.into_owned())
                .unwrap_or_default();
            if sensitive(&decoded) {
                format!("{key}=[redacted]")
            } else {
                part.into()
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}
fn secret_values(value: &Value) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    fn visit(v: &Value, out: &mut Vec<Vec<u8>>) {
        match v {
            Value::Object(map) => {
                for (key, value) in map {
                    if sensitive(key)
                        && let Some(s) = value.as_str()
                        && !s.is_empty()
                    {
                        out.push(s.as_bytes().to_vec());
                    } else {
                        visit(value, out);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, out);
                }
            }
            _ => {}
        }
    }
    visit(value, &mut out);
    out
}
/// Also mask possible fragments at chunk boundaries; never buffer the stream.
fn redact_bytes(bytes: &[u8], secrets: &[Vec<u8>]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for secret in secrets {
        if !bytes.is_empty() && secret.windows(bytes.len()).any(|part| part == bytes) {
            out.fill(b'*');
        }
        for (i, window) in bytes.windows(secret.len()).enumerate() {
            if window == secret {
                out[i..i + secret.len()].fill(b'*');
            }
        }
        for n in 1..secret.len().min(bytes.len() + 1) {
            if bytes.ends_with(&secret[..n]) {
                out[bytes.len() - n..].fill(b'*');
            }
            if bytes.starts_with(&secret[secret.len() - n..]) {
                out[..n].fill(b'*');
            }
        }
    }
    out
}
fn usage_json(u: &NormalizedUsage) -> Value {
    let t = &u.tokens;
    json!({"tokens": {
        "input_tokens": t.input_tokens, "output_tokens": t.output_tokens,
        "cached_input_tokens": t.cached_input_tokens, "cache_creation_5m_tokens": t.cache_creation_5m_tokens,
        "cache_creation_30m_tokens": t.cache_creation_30m_tokens, "cache_creation_1h_tokens": t.cache_creation_1h_tokens,
        "reasoning_tokens": t.reasoning_tokens,
    }, "metrics": u.metrics, "dimensions": u.dimensions, "actual_service_tier": u.actual_service_tier,
    "completeness": format!("{:?}", u.completeness).to_lowercase(),
    "attempts": u.attempts.iter().map(|a| json!({"model": a.model, "usage": usage_json(&a.usage), "billable": a.billable, "started_at_ms": a.started_at_ms})).collect::<Vec<_>>(),
    "responses": u.responses.iter().map(|r| json!({"id": r.id, "usage": usage_json(&r.usage)})).collect::<Vec<_>>()})
}
