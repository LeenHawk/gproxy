//! Store-backed upstream capture and per-call usage. One writer per exchange
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
    capture::{Chunk, Segment, Segments, tenant_scope},
    entity::usage::{
        capture_link, upstream_event as event, upstream_record as record, usage_record,
    },
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
            usage: (s.settlement || s.usage) && request.operation.operation.produces_usage(),
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
            kind: Set(record::CaptureKind::Http),
            user_id: reported(request.attribution.user_id.clone()),
            api_key_id: reported(request.attribution.api_key_id.clone()),
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
        let link = request.request_id.clone();
        crate::rt::spawn(async move {
            write_capture(store, row, rx, worker_lost, full, cancellation, link).await;
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
            if !request.snapshot.observation.usage || !request.operation.operation.produces_usage()
            {
                return;
            }
            let backend = self.store.connection().get_database_backend();
            let mut statements = Vec::with_capacity(report.exchanges.len() * 2);
            for exchange in &report.exchanges {
                let row = usage_record::ActiveModel {
                    request_id: Set(exchange.capture_id.clone()),
                    user_id: reported(request.attribution.user_id.clone()),
                    api_key_id: reported(request.attribution.api_key_id.clone()),
                    model: Set(exchange.upstream_model.clone().unwrap_or_default()),
                    operation: Set(request.operation.operation.id().into()),
                    provider_id: Set(Some(exchange.provider_id.clone())),
                    credential_id: Set(Some(exchange.credential_id.clone())),
                    attempt_id: Set(Some(exchange.attempt_id.clone())),
                    attempt_ordinal: Set(Some(i64::from(exchange.attempt_ordinal))),
                    state: Set(Some(format!("{:?}", report.state).to_lowercase())),
                    cost: reported(
                        exchange
                            .cost
                            .as_ref()
                            .and_then(|c| FixedDecimal::rounded(c.amount).ok()),
                    ),
                    started_at_ms: Set(request.started_at_ms),
                    ended_at_ms: Set(Some(now_ms())),
                    ..usage_columns(&exchange.usage)
                };
                statements.push(usage_record::Entity::insert(row).build(backend));
                statements.push(link_statement(
                    backend,
                    &report.request_id,
                    &exchange.capture_id,
                ));
            }
            if let Err(error) = self.store.connection().atomic_batch_owned(statements).await {
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
    Event(Chunk),
    Finish(CaptureEnd, oneshot::Sender<()>),
    Flush(oneshot::Sender<()>),
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
        turn_id: Option<&str>,
    ) {
        if !self.full {
            return;
        }
        let payload = if self.redact {
            redact_bytes(payload, &self.secrets)
        } else {
            payload.to_vec()
        };
        self.send(Write::Event(Chunk {
            sequence: sequence as i64,
            turn_id: turn_id.map(str::to_owned),
            direction,
            kind,
            payload,
            observed_at_ms: now_ms(),
        }));
    }
}
impl CaptureSink for StoreCapture {
    fn flush(&self) -> CapabilityFuture<'static, ()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Write::Flush(tx));
        Box::pin(async move {
            let _ = rx.await;
        })
    }
    fn record(&mut self, sequence: u64, event: CaptureEvent<'_>) {
        use event::{CaptureDirection as D, CaptureEventKind as K};
        let mut patch = record::ActiveModel {
            id: Set(self.id.clone()),
            ..Default::default()
        };
        match event {
            CaptureEvent::TurnStart {
                connection_id,
                lane,
            } => {
                patch.kind = Set(record::CaptureKind::WsTurn);
                patch.session_id = Set(Some(connection_id.to_owned()));
                patch.stream_key = Set(lane.map(str::to_owned));
                patch.request_framing = Set(record::BodyFraming::WebSocket);
                patch.response_framing = Set(record::BodyFraming::WebSocket);
            }
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
                self.bytes(sequence, D::Request, K::Bytes, bytes, None);
                return;
            }
            CaptureEvent::ResponseChunk(bytes) => {
                self.bytes(sequence, D::Response, K::Bytes, bytes, None);
                return;
            }
            CaptureEvent::Frame {
                direction,
                frame,
                turn_id,
            } => {
                let direction = match direction {
                    crate::CaptureDirection::Request => D::Request,
                    crate::CaptureDirection::Response => D::Response,
                };
                match frame {
                    WsFrame::Text(s) => {
                        self.bytes(sequence, direction, K::WsText, s.as_bytes(), turn_id)
                    }
                    WsFrame::Binary(b) => self.bytes(sequence, direction, K::WsBinary, b, turn_id),
                    WsFrame::Ping(b) => self.bytes(sequence, direction, K::WsPing, b, turn_id),
                    WsFrame::Pong(b) => self.bytes(sequence, direction, K::WsPong, b, turn_id),
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
                        turn_id,
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
    fn reason(&mut self, reason: gproxy_channel::channel::ResponseReason) {
        self.send(Write::Head(Box::new(record::ActiveModel {
            id: Set(self.id.clone()),
            reason: Set(Some(reason.as_str().to_owned())),
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
/// Only the background writer buffers/compresses captured bytes. The capture
/// sink redacts and enqueues owned chunks without delaying the forwarded stream.
async fn write_capture<C: BatchConnectionTrait + Send + Sync + 'static>(
    store: Arc<Store<C>>,
    mut row: record::ActiveModel,
    mut rx: mpsc::UnboundedReceiver<Write>,
    lost: Arc<AtomicBool>,
    full: bool,
    cancellation: tokio_util::sync::CancellationToken,
    link: String,
) {
    let id = row.id.clone().unwrap();
    let scope = tenant_scope(
        row.api_key_id
            .try_as_ref()
            .and_then(|value| value.as_deref()),
        row.user_id.try_as_ref().and_then(|value| value.as_deref()),
        &format!("upstream:{id}"),
    );
    let mut inserted = false;
    let mut segments = Segments::default();
    let mut ack = None;
    let mut end = CaptureEnd::Interrupted;
    let mut status = None;
    loop {
        // Sleep until the next chunk when nothing is pending; otherwise wake
        // when the oldest pending segment goes idle so it is flushed on time.
        let write = match segments.deadline() {
            Some(deadline) => crate::rt::timeout(deadline, rx.recv()).await,
            None => Some(rx.recv().await),
        };
        let (ready, flush) = match write {
            None => (segments.drain(false), None),
            Some(None) => break,
            Some(Some(Write::Flush(ack))) => (segments.drain(true), Some(ack)),
            Some(Some(Write::Head(patch))) => {
                if let sea_orm::ActiveValue::Set(s) = &patch.response_status {
                    status = *s;
                }
                merge(&mut row, *patch);
                continue;
            }
            Some(Some(Write::Event(event))) => (segments.push(event), None),
            Some(Some(Write::Finish(value, sender))) => {
                end = value;
                ack = Some(sender);
                break;
            }
        };
        if (!ready.is_empty() || flush.is_some())
            && let Err(error) = persist_capture(
                &store,
                &row,
                &mut inserted,
                ready,
                &link,
                &scope,
                flush.is_some(),
            )
            .await
        {
            lost.store(true, Ordering::Relaxed);
            tracing::error!(capture_id = %id, %error, "capture write failed");
        }
        if let Some(ack) = flush {
            let _ = ack.send(());
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
    row.state = Set(state);
    row.ended_at_ms = Set(Some(now_ms()));
    row.request_body_state = Set(
        if full && status.is_some() && status != Some(101) && !incomplete {
            record::CaptureBodyState::Complete
        } else {
            body_state
        },
    );
    row.response_body_state = Set(body_state);
    row.error = Set(if incomplete {
        Some("capture writer unavailable or database write failure".into())
    } else {
        match end {
            CaptureEnd::Complete => None,
            CaptureEnd::Interrupted => Some("upstream exchange interrupted".into()),
            CaptureEnd::Cancelled => Some("request cancelled or response dropped".into()),
        }
    });
    if let Err(error) = persist_capture(
        &store,
        &row,
        &mut inserted,
        segments.drain(true),
        &link,
        &scope,
        true,
    )
    .await
    {
        lost.store(true, Ordering::Relaxed);
        tracing::error!(capture_id = %id, %error, "capture finalization failed");
    }
    if let Some(ack) = ack {
        let _ = ack.send(());
    }
}

async fn persist_capture<C: BatchConnectionTrait>(
    store: &Store<C>,
    row: &record::ActiveModel,
    inserted: &mut bool,
    segments: Vec<Segment>,
    link: &str,
    scope: &str,
    write_head: bool,
) -> gproxy_store::Result<()> {
    let id = row.id.clone().unwrap();
    let mut statements = if !*inserted || write_head {
        store.capture_upstream_head(row.clone(), *inserted)?
    } else {
        Vec::new()
    };
    if !*inserted {
        statements.push(link_statement(
            store.connection().get_database_backend(),
            link,
            &id,
        ));
    }
    for segment in segments {
        statements.extend(store.capture_upstream_segment(&id, scope, segment)?);
    }
    store.connection().atomic_batch_owned(statements).await?;
    *inserted = true;
    Ok(())
}

/// Copy every column `patch` sets onto `row`.
fn merge(row: &mut record::ActiveModel, patch: record::ActiveModel) {
    use sea_orm::{ActiveModelTrait, Iterable};
    for column in record::Column::iter() {
        if let sea_orm::ActiveValue::Set(value) = patch.get(column) {
            row.set(column, value);
        }
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
/// Also mask possible fragments at chunk boundaries; never buffer the forwarded
/// stream. Redacted log bytes may be coalesced by the background writer.
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

/// Store fixed fields as columns; avoid building the full usage JSON on the write path.
fn usage_columns(u: &NormalizedUsage) -> usage_record::ActiveModel {
    let mut extra = serde_json::Map::new();
    let mut overflow_tokens = serde_json::Map::new();
    let mut token = |name: &str, value: Option<u64>| {
        value.and_then(|value| match i64::try_from(value) {
            Ok(value) => Some(value),
            Err(_) => {
                overflow_tokens.insert(name.into(), json!(value));
                None
            }
        })
    };
    let input_tokens = token("input_tokens", u.tokens.input_tokens);
    let output_tokens = token("output_tokens", u.tokens.output_tokens);
    let cached_input_tokens = token("cached_input_tokens", u.tokens.cached_input_tokens);
    let cache_creation_5m_tokens = token(
        "cache_creation_5m_tokens",
        u.tokens.cache_creation_5m_tokens,
    );
    let cache_creation_30m_tokens = token(
        "cache_creation_30m_tokens",
        u.tokens.cache_creation_30m_tokens,
    );
    let cache_creation_1h_tokens = token(
        "cache_creation_1h_tokens",
        u.tokens.cache_creation_1h_tokens,
    );
    let reasoning_tokens = token("reasoning_tokens", u.tokens.reasoning_tokens);
    if !overflow_tokens.is_empty() {
        extra.insert("tokens".into(), Value::Object(overflow_tokens));
    }
    let mut quantities = u.metrics.clone();
    // Never round a reported quantity. Keep unrepresentable values in extensions.
    let mut take = |key: &str| {
        let value = FixedDecimal::exact(*quantities.get(key)?).ok()?;
        quantities.remove(key);
        Some(value)
    };
    let image_input_tokens = take("image_input_tokens");
    let image_output_tokens = take("image_output_tokens");
    let image_outputs = take("image_outputs");
    let audio_input_tokens = take("audio_input_tokens");
    let cached_audio_input_tokens = take("cached_audio_input_tokens");
    let audio_output_tokens = take("audio_output_tokens");
    let audio_seconds = take("audio_seconds");
    let audio_characters = take("audio_characters");
    let video_input_tokens = take("video_input_tokens");
    let video_tokens = take("video_tokens");
    let video_seconds = take("video_seconds");
    let video_outputs = take("video_outputs");
    let search_units = take("search_units");
    let web_searches = take("web_searches");
    let web_fetches = take("web_fetches");
    let file_searches = take("file_searches");
    let code_interpreter_sessions = take("code_interpreter_sessions");
    let tool_calls = take("tool_calls");
    let requests = take("requests");
    if !quantities.is_empty() {
        extra.insert("metrics".into(), json!(quantities));
    }
    if !u.dimensions.is_empty() {
        extra.insert("dimensions".into(), json!(u.dimensions));
    }
    if !u.attempts.is_empty() {
        extra.insert("attempts".into(), json!(u.attempts.iter().map(|a| json!({"model": a.model, "usage": usage_json(&a.usage), "billable": a.billable, "started_at_ms": a.started_at_ms})).collect::<Vec<_>>()));
    }
    if !u.responses.is_empty() {
        extra.insert(
            "responses".into(),
            json!(
                u.responses
                    .iter()
                    .map(|r| json!({"id": r.id, "usage": usage_json(&r.usage)}))
                    .collect::<Vec<_>>()
            ),
        );
    }
    usage_record::ActiveModel {
        input_tokens: reported(input_tokens),
        output_tokens: reported(output_tokens),
        cached_input_tokens: reported(cached_input_tokens),
        cache_creation_5m_tokens: reported(cache_creation_5m_tokens),
        cache_creation_30m_tokens: reported(cache_creation_30m_tokens),
        cache_creation_1h_tokens: reported(cache_creation_1h_tokens),
        reasoning_tokens: reported(reasoning_tokens),
        image_input_tokens: reported(image_input_tokens),
        image_output_tokens: reported(image_output_tokens),
        image_outputs: reported(image_outputs),
        audio_input_tokens: reported(audio_input_tokens),
        cached_audio_input_tokens: reported(cached_audio_input_tokens),
        audio_output_tokens: reported(audio_output_tokens),
        audio_seconds: reported(audio_seconds),
        audio_characters: reported(audio_characters),
        video_input_tokens: reported(video_input_tokens),
        video_tokens: reported(video_tokens),
        video_seconds: reported(video_seconds),
        video_outputs: reported(video_outputs),
        search_units: reported(search_units),
        web_searches: reported(web_searches),
        web_fetches: reported(web_fetches),
        file_searches: reported(file_searches),
        code_interpreter_sessions: reported(code_interpreter_sessions),
        tool_calls: reported(tool_calls),
        requests: reported(requests),
        completeness: Set(Some(format!("{:?}", u.completeness).to_lowercase())),
        actual_service_tier: reported(u.actual_service_tier.clone()),
        metrics: Set(Value::Object(extra)),
        ..Default::default()
    }
}

/// Nullable quantity/identity columns default to NULL. Omit unreported fields
/// from INSERT instead of constructing and binding dozens of NULL parameters.
fn reported<T>(value: Option<T>) -> sea_orm::ActiveValue<Option<T>>
where
    Option<T>: Into<sea_orm::Value>,
{
    match value {
        Some(value) => Set(Some(value)),
        None => sea_orm::ActiveValue::NotSet,
    }
}

fn link_statement(
    backend: sea_orm::DbBackend,
    downstream_id: &str,
    upstream_id: &str,
) -> sea_orm::Statement {
    capture_link::Entity::insert(capture_link::ActiveModel {
        downstream_id: Set(downstream_id.to_owned()),
        upstream_id: Set(upstream_id.to_owned()),
    })
    .on_conflict(
        sea_orm::sea_query::OnConflict::columns([
            capture_link::Column::DownstreamId,
            capture_link::Column::UpstreamId,
        ])
        .do_nothing()
        .to_owned(),
    )
    .build(backend)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod storage_tests {
    use super::*;
    use gproxy_store::capture::{compress, decompress};

    #[test]
    fn redaction_happens_before_the_writer_compresses_bytes() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sink = StoreCapture {
            id: "redaction".into(),
            tx,
            lost: Arc::new(AtomicBool::new(false)),
            full: true,
            redact: true,
            secrets: vec![b"secret-sentinel".to_vec()],
        };
        let original = b"data: {\"token\":\"secret-sentinel\"}\n\n".repeat(3000);
        sink.bytes(
            1,
            event::CaptureDirection::Response,
            event::CaptureEventKind::Bytes,
            &original,
            None,
        );
        let Write::Event(chunk) = rx.try_recv().unwrap() else {
            panic!("expected captured chunk")
        };
        assert!(!chunk.payload.windows(15).any(|w| w == b"secret-sentinel"));
        let (encoding, stored) = compress(&chunk.payload).unwrap();
        assert_eq!(encoding, "zstd");
        assert_eq!(decompress(&encoding, &stored).unwrap(), chunk.payload);
        // The source bytes, which the caller forwards, were never changed.
        assert!(original.windows(15).any(|w| w == b"secret-sentinel"));
    }

    #[tokio::test]
    async fn idle_and_explicit_flush_persist_pending_segments_before_finish() {
        let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).sqlx_logging(false);
        let store = Arc::new(Store::new(
            sea_orm::Database::connect(options).await.unwrap(),
        ));
        store.sync().await.unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let worker = tokio::spawn(write_capture(
            store.clone(),
            record::ActiveModel {
                id: Set("idle".into()),
                kind: Set(record::CaptureKind::Http),
                started_at_ms: Set(1),
                ..Default::default()
            },
            rx,
            Arc::new(AtomicBool::new(false)),
            true,
            tokio_util::sync::CancellationToken::new(),
            "downstream".into(),
        ));
        tx.send(Write::Event(Chunk {
            sequence: 0,
            turn_id: None,
            direction: event::CaptureDirection::Response,
            kind: event::CaptureEventKind::Bytes,
            payload: b"first".to_vec(),
            observed_at_ms: 1,
        }))
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if !store
                    .upstream_events()
                    .query(event::Entity::find())
                    .await
                    .unwrap()
                    .is_empty()
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            store
                .upstream_records()
                .get_many(&["idle".into()])
                .await
                .unwrap()[0]
                .as_ref()
                .unwrap()
                .ended_at_ms
                .is_none()
        );
        tx.send(Write::Event(Chunk {
            sequence: 1,
            turn_id: None,
            direction: event::CaptureDirection::Response,
            kind: event::CaptureEventKind::Bytes,
            payload: b"second".to_vec(),
            observed_at_ms: 2,
        }))
        .unwrap();
        let (ack, done) = oneshot::channel();
        tx.send(Write::Flush(ack)).unwrap();
        done.await.unwrap();
        assert_eq!(
            store
                .upstream_events()
                .query(event::Entity::find())
                .await
                .unwrap()
                .len(),
            2
        );
        let (ack, done) = oneshot::channel();
        tx.send(Write::Finish(CaptureEnd::Complete, ack)).unwrap();
        done.await.unwrap();
        worker.await.unwrap();
    }
}
