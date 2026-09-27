//! Downstream request/response capture: the `downstream_records` row for the
//! caller's own exchange. Independent capture links associate upstream calls with downstream requests.
//!
//! # Why this side is the host's
//!
//! Core's `StoreObserver` writes one upstream record per physical
//! send and its `usage_records` row. It deliberately writes no
//! downstream record at all, because only a host sees the inbound HTTP
//! exchange: `design/core-observation.md` says it outright — the host owns the
//! downstream record, and core never fabricates one. This module is that half.
//!
//! The two halves agree by construction rather than by convention:
//!
//! | | upstream (core) | downstream (here) |
//! |---|---|---|
//! | gate | `enable_upstream_log` | `enable_downstream_log` |
//! | body gate | `enable_upstream_log_body` | `enable_downstream_log_body` |
//! | redaction | `disable_log_redaction` | the same switch, the same field list |
//! | id | its own, opaque | **the downstream request id** |
//! | body storage | `upstream_events`, streamed | the inline column, buffered |
//! | websocket frames | `upstream_events`, per frame | `downstream_events`, per frame, buffered |
//! | failure | logged, never fails the request | logged, never fails the request |
//!
//! # Ask before you clone
//!
//! `design/crates.md` puts it as "a capability core queries before doing the
//! work, not a sink it throws things away into". So the switches are read
//! first and nothing is copied that will not be stored: with
//! `enable_downstream_log` off [`DownstreamCapture::open`] answers `None` and
//! there is no capture at all, and with `enable_downstream_log_body` off the
//! request body is never copied and [`DownstreamCapture::record_response_chunk`]
//! returns immediately. When enabled, received bodies and frames are retained
//! without a logging-size cutoff.
//!
//! # What a downstream record does not claim
//!
//! `provider_id` and `credential_id` stay unset. A request that was
//! retried reached two providers with two credentials, and a column that can
//! hold one of them would have to pick; each upstream record carries its own
//! attribution and associated downstream request ID. `metrics` stays unset too — the schema
//! reserves it for upstream-native usage, and physical usage is stored independently in
//! `usage_records`.

use std::borrow::Cow;

use gproxy_core::UsageCompletion;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Store,
    entity::{
        config::setting,
        usage::{downstream_event as event, downstream_record as record},
    },
};
use http::{HeaderMap, StatusCode};
use sea_orm::Set;
use serde_json::{Value, json};

use crate::{Admitted, AppError, Caller, DataPlaneRequest, now_ms};

/// The two directions a captured frame or chunk travelled in, as the schema
/// spells them. Re-exported so a host can name one without depending on
/// `gproxy-store`.
pub use event::{CaptureDirection, CaptureEventKind};

/// What a redacted value is replaced by. The same marker core writes, so the
/// two sides of one request read alike.
const REDACTED: &str = "[redacted]";

/// The logging switches of the active configuration revision.
///
/// The counterpart of `gproxy_core::ObservationSettings`, read from the same
/// `settings` row at the same reload, and carried on [`AppData`] so a request
/// decides against the revision it pinned rather than against whatever is
/// current by the time its response finishes.
///
/// [`AppData`]: crate::AppData
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationSwitches {
    /// `settings.enable_downstream_log`. Off means no record, no clone, no
    /// write.
    pub downstream_log: bool,
    /// `settings.enable_downstream_log_body`. Off means headers and timings
    /// only; the bodies are not copied.
    pub downstream_log_body: bool,
    /// `settings.enable_upstream_log`, which core reads for its own half.
    /// Read here for one reason: with it off there are no upstream records,
    /// so an edge would point at a row that was never written.
    pub upstream_log: bool,
    /// `!settings.disable_log_redaction`, spelled the way core spells it.
    pub redact: bool,
}

impl Default for ObservationSwitches {
    /// The schema's own defaults: log nothing until an operator asks for it,
    /// and redact what is logged. Each logged exchange is two more records
    /// written per request, a cost a gateway should not pay unasked. Identical to `ObservationSettings::default()`, so an instance
    /// with no settings row behaves the same on both sides.
    fn default() -> Self {
        Self {
            downstream_log: false,
            downstream_log_body: false,
            upstream_log: false,
            redact: true,
        }
    }
}

impl ObservationSwitches {
    pub fn from_settings(settings: Option<&setting::Model>) -> Self {
        settings.map_or_else(Self::default, |row| Self {
            downstream_log: row.enable_downstream_log,
            downstream_log_body: row.enable_downstream_log_body,
            upstream_log: row.enable_upstream_log,
            redact: !row.disable_log_redaction,
        })
    }
}

/// How the downstream exchange ended, as the host saw it.
///
/// Independent of the HTTP status: a captured `500` that reached the client
/// intact is [`CaptureOutcome::Complete`], and a `200` whose stream stopped
/// half way is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// The whole response was written to the client.
    Complete,
    /// The response stopped before it was finished, and not because the
    /// client asked it to.
    Interrupted,
    /// The client went away, or the response was dropped unread.
    Cancelled,
    /// Nothing was answered: the call itself failed.
    Failed { error: String },
}

/// One websocket message, as a host saw it on the wire.
///
/// Borrowed, so a frame that is not going to be stored costs nothing: with
/// `enable_downstream_log_body` off [`DownstreamCapture::record_frame`]
/// returns before it reads the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturedFrame<'a> {
    Text(&'a str),
    Binary(&'a [u8]),
    Ping(&'a [u8]),
    Pong(&'a [u8]),
    /// The payload column holds the close code and reason, which is what the
    /// schema documents for `ws_close`.
    Close {
        code: Option<u16>,
        reason: &'a str,
    },
}

impl CapturedFrame<'_> {
    /// The stored kind and payload. `Close` is rendered as the same JSON
    /// object core's observer writes, so the two sides of one socket read
    /// alike.
    fn encode(self) -> (CaptureEventKind, Vec<u8>) {
        match self {
            Self::Text(text) => (CaptureEventKind::WsText, text.as_bytes().to_vec()),
            Self::Binary(bytes) => (CaptureEventKind::WsBinary, bytes.to_vec()),
            Self::Ping(bytes) => (CaptureEventKind::WsPing, bytes.to_vec()),
            Self::Pong(bytes) => (CaptureEventKind::WsPong, bytes.to_vec()),
            Self::Close { code, reason } => {
                let value = code.map(|code| json!({"code": code, "reason": reason}));
                (
                    CaptureEventKind::WsClose,
                    serde_json::to_vec(&value).unwrap_or_else(|_| b"null".to_vec()),
                )
            }
        }
    }
}

/// One downstream exchange, buffered until it ends.
///
/// Created by [`App::call`](crate::App::call) and
/// [`App::connect`](crate::App::connect) and handed back inside the outcome,
/// because the response they return has not been written yet — a stream is
/// still running long after `call()` returned, and only the host knows when it
/// stopped and why.
///
/// # What a host does with it
///
/// ```ignore
/// let CallOutcome { execution, admitted, mut capture } = app.call(&caller, request).await?;
/// let (response, usage) = execution.into_parts();          // the head is already recorded
/// while let Some(chunk) = body.next().await {
///     if let Some(capture) = capture.as_mut() {
///         capture.record_response_chunk(&chunk);
///     }
///     sink.send(chunk).await?;
/// }
/// if let Some(capture) = capture {
///     // Awaits settlement and writes the record and events in one batch.
///     let _ = capture.settle(app.gproxy().store(), CaptureOutcome::Complete, usage).await;
/// }
/// drop(admitted);
/// ```
///
/// A host that needs the `UsageReport` itself awaits the `UsageCompletion`,
/// then calls [`DownstreamCapture::finish`]. [`DownstreamCapture::settle`]
/// awaits it for a host that does not need the report itself.
///
/// # Nothing is written until it ends
///
/// Unlike core's observer, which creates an `in_progress` row and then
/// streams into it, this buffers and writes once. A downstream exchange is one
/// row plus events, so a second round trip would only create a placeholder. The cost is that a process
/// killed mid-stream leaves no downstream row for that request — the upstream
/// records and their `initiator_request_id` still show it happened.
pub struct DownstreamCapture {
    id: String,
    switches: ObservationSwitches,
    /// The row as it is known so far. Mutated in place by the response
    /// recorders, exactly as core's observer patches its own.
    row: record::ActiveModel,
    /// Received response bytes for the inline column.
    response_body: Vec<u8>,
    /// Kept out of the row so the final state can be decided from it.
    status: Option<i32>,
    /// One row per websocket message, in the order the socket saw them across
    /// both directions. Empty for an HTTP exchange, which uses the inline
    /// body columns instead.
    events: Vec<event::ActiveModel>,
}

impl std::fmt::Debug for DownstreamCapture {
    /// Identity and sizes. The row holds the caller's headers and prompt.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownstreamCapture")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("response_bytes", &self.response_body.len())
            .finish_non_exhaustive()
    }
}

impl DownstreamCapture {
    /// Start capturing, or answer `None` when `enable_downstream_log` is off.
    ///
    /// `None` is the whole of the "off" behaviour: no allocation, no clone of
    /// the body, no row. The request parts are read here rather than later
    /// because this runs before the engine is handed anything.
    ///
    /// Every attribution column comes from `caller` and `admitted` — the two
    /// values admission decided from — and none of them from the request's own
    /// headers. A client that sends `x-user-id` does not get to say whose log
    /// line this is.
    ///
    /// A request refused by admission therefore has no downstream record:
    /// there is no `Admitted` to attribute one to. The refusal is the host's
    /// access log, not this table's.
    pub fn open(
        switches: &ObservationSwitches,
        request: &DataPlaneRequest,
        caller: &Caller,
        admitted: &Admitted,
    ) -> Option<Self> {
        Self::open_exchange(
            switches,
            &request.request_id,
            &request.parts,
            &request.body,
            caller,
            admitted.attribution.model.clone(),
            request.operation.operation.id(),
            request.client_ip.clone(),
        )
    }

    /// A channel service is logged without charging a model-call admission lease.
    pub fn open_service(
        switches: &ObservationSwitches,
        request: &crate::ServiceRequestIn,
        caller: &Caller,
        client_ip: Option<String>,
    ) -> Option<Self> {
        let mut capture = Self::open_exchange(
            switches,
            &request.request_id,
            &request.parts,
            &request.body,
            caller,
            None,
            "service",
            client_ip,
        )?;
        capture.row.provider_id = Set(Some(request.provider_id.clone()));
        Some(capture)
    }

    #[allow(clippy::too_many_arguments)]
    fn open_exchange(
        switches: &ObservationSwitches,
        request_id: &str,
        parts: &http::request::Parts,
        body: &[u8],
        caller: &Caller,
        model: Option<String>,
        operation: &str,
        client_ip: Option<String>,
    ) -> Option<Self> {
        if !switches.downstream_log {
            return None;
        }
        let request_body = capture_body(body, switches);
        let uri = &parts.uri;
        let row = record::ActiveModel {
            id: Set(request_id.to_owned()),
            // Promoted to `WsConnection` by `record_response_head` if the
            // handshake is accepted.
            kind: Set(record::CaptureKind::Http),
            // `Caller::user_id` rather than the attribution's: the attribution
            // makes it optional for core's benefit, and here it is always
            // known.
            user_id: Set(Some(caller.user_id.clone())),
            api_key_id: Set(caller.api_key_id.clone()),
            // The name the client asked for, which is what a log is read
            // against. The upstream name a route resolved it to is on the
            // upstream records.
            model: Set(model),
            operation: Set(Some(operation.into())),
            request_method: Set(Some(parts.method.to_string())),
            request_url: Set(Some(uri.path().to_owned())),
            request_query: Set(uri.query().map(|query| {
                if switches.redact {
                    redact_query(query)
                } else {
                    query.to_owned()
                }
            })),
            request_headers: Set(Some(headers_json(&parts.headers, switches.redact))),
            request_body: Set(request_body),
            // Both directions use the inline column, which the schema spells
            // `Buffered`; the other framings mean "read the events instead",
            // and this side writes none.
            request_framing: Set(record::BodyFraming::Buffered),
            response_framing: Set(record::BodyFraming::Buffered),
            client_ip: Set(client_ip),
            state: Set(record::CaptureState::InProgress),
            started_at_ms: Set(now_ms()),
            ..Default::default()
        };
        Some(Self {
            id: request_id.to_owned(),
            switches: *switches,
            row,
            response_body: Vec::new(),
            status: None,
            events: Vec::new(),
        })
    }

    /// The capture id, which is the downstream request id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Record the status and headers the client is about to be sent.
    ///
    /// [`App::call`](crate::App::call) already did this with the response it
    /// returned, so an HTTP host only calls it if it rewrote the head. A
    /// websocket host calls it itself with `101` once it has accepted the
    /// upgrade, which is what turns the record into a `WsConnection`.
    pub fn record_response_head(&mut self, status: StatusCode, headers: &HeaderMap) {
        let code = i32::from(status.as_u16());
        self.status = Some(code);
        self.row.response_status = Set(Some(code));
        self.row.response_headers = Set(Some(headers_json(headers, self.switches.redact)));
        self.row.first_response_at_ms = Set(Some(now_ms()));
        if status == StatusCode::SWITCHING_PROTOCOLS {
            self.row.kind = Set(record::CaptureKind::WsConnection);
        }
    }

    /// Append every received response byte when body capture is enabled.
    ///
    /// A no-op with `enable_downstream_log_body` off, so a host can call it
    /// unconditionally on every chunk without checking the switch. Redaction
    /// runs once over the assembled buffer at the end rather than per chunk:
    /// it is key-based over JSON, which needs the whole document.
    pub fn record_response_chunk(&mut self, bytes: &[u8]) {
        if !self.switches.downstream_log_body || bytes.is_empty() {
            return;
        }
        self.response_body.extend_from_slice(bytes);
    }

    /// One websocket message, recorded as a `downstream_events` row.
    ///
    /// The schema decides the shape, not this crate: "all WS messages append
    /// to the WsConnection", and `sequence` is "host-assigned monotonic order
    /// across both directions of ... the entire WS connection, including
    /// control messages and concurrent turns". So a socket is **one** record
    /// with an ordered event list, not one record per exchange.
    ///
    /// `turn_id` is left unset, and deliberately. The column identifies "the
    /// WS business turn, when identifiable", and a turn is a dialect's notion
    /// — OpenAI's `response.created`/`response.done`, Gemini Live's own —
    /// while this host forwards realtime frames as opaque passthrough (core
    /// refuses to convert any websocket dialect but
    /// `OpenAiResponsesWebSocket`). Inventing a boundary the wire did not draw
    /// would put a `WsTurn` record in the log that nothing produced, so the
    /// connection is recorded whole and the turn column stays `None`.
    ///
    /// A no-op with `enable_downstream_log_body` off, exactly like
    /// [`DownstreamCapture::record_response_chunk`]: a frame *is* the body of
    /// a socket, so it is gated by the body switch rather than by a third one.
    pub fn record_frame(&mut self, direction: CaptureDirection, frame: CapturedFrame<'_>) {
        if !self.switches.downstream_log_body {
            return;
        }
        // The socket's bodies are the events, not the inline columns. Said on
        // the first frame rather than at the handshake, so a socket that
        // carried nothing still reads as an empty buffered exchange.
        self.row.request_framing = Set(record::BodyFraming::WebSocket);
        self.row.response_framing = Set(record::BodyFraming::WebSocket);
        let (kind, payload) = frame.encode();
        let payload = redacted(&payload, self.switches.redact).into_owned();
        self.events.push(event::ActiveModel {
            capture_id: Set(self.id.clone()),
            sequence: Set(self.events.len() as i64),
            turn_id: Set(None),
            direction: Set(direction),
            kind: Set(kind),
            payload: Set(payload),
            observed_at_ms: Set(now_ms()),
        });
    }

    /// Await usage settlement before persisting the downstream log and events.
    pub async fn settle<C: BatchConnectionTrait + Send + Sync>(
        self,
        store: &Store<C>,
        outcome: CaptureOutcome,
        usage: UsageCompletion,
    ) -> Result<String, AppError> {
        if let Err(error) = usage.await {
            tracing::warn!(capture_id = %self.id, %error, "settlement failed; recording downstream exchange");
        }
        self.finish(store, outcome).await
    }

    /// Write the downstream record and its events atomically. Upstream records
    /// already carry their downstream request ID; no lookup or edge write is needed.
    /// Persistence failures are logged and must not replace the caller's response.
    pub async fn finish<C: BatchConnectionTrait + Send + Sync>(
        self,
        store: &Store<C>,
        outcome: CaptureOutcome,
    ) -> Result<String, AppError> {
        let id = self.id.clone();
        let statements = self.into_statements(store, outcome)?;
        store
            .connection()
            .atomic_batch_owned(statements)
            .await
            .map_err(|error| {
                tracing::error!(capture_id = %id, %error, "downstream capture write failed");
                AppError::Store(error.into())
            })?;
        Ok(id)
    }

    /// Insert the record before its events, whose foreign key references it.
    fn into_statements<C: BatchConnectionTrait>(
        self,
        store: &Store<C>,
        outcome: CaptureOutcome,
    ) -> Result<Vec<sea_orm::Statement>, AppError> {
        let Self {
            id: _,
            switches,
            mut row,
            response_body,
            status,
            events,
        } = self;

        let (state, error) = match &outcome {
            // Capture completeness is not HTTP success: a refusal that
            // reached the client whole is a failed exchange, recorded whole.
            CaptureOutcome::Complete if status.is_some_and(|code| code >= 400) => {
                (record::CaptureState::Failed, None)
            }
            CaptureOutcome::Complete => (record::CaptureState::Completed, None),
            CaptureOutcome::Interrupted => (
                record::CaptureState::Failed,
                Some("downstream response interrupted".to_owned()),
            ),
            CaptureOutcome::Cancelled => (
                record::CaptureState::Cancelled,
                Some("client disconnected or the response was dropped".to_owned()),
            ),
            CaptureOutcome::Failed { error } => (record::CaptureState::Failed, Some(error.clone())),
        };

        let response_body = switches
            .downstream_log_body
            .then(|| redacted(&response_body, switches.redact).into_owned());

        row.state = Set(state);
        row.error = Set(error);
        row.ended_at_ms = Set(Some(now_ms()));
        row.request_body_state = Set(body_state(switches.downstream_log_body, true));
        row.response_body_state = Set(body_state(
            switches.downstream_log_body,
            outcome == CaptureOutcome::Complete,
        ));
        row.response_body = Set(response_body);

        let mut statements = vec![store.downstream_records().insert_statement(row)?];
        // Every frame the socket carried, in observed order, after the record
        // they belong to: `downstream_events.capture_id` is
        // a foreign key onto the row just inserted.
        for event in events {
            statements.push(store.downstream_events().insert_statement(event)?);
        }
        Ok(statements)
    }
}

/// `NotCaptured` when the switch is off, `Partial` when what is stored is not
/// the whole of what passed, `Complete` otherwise.
fn body_state(captured: bool, whole: bool) -> record::CaptureBodyState {
    if !captured {
        record::CaptureBodyState::NotCaptured
    } else if !whole {
        record::CaptureBodyState::Partial
    } else {
        record::CaptureBodyState::Complete
    }
}

/// The complete request body under the configured redaction policy.
fn capture_body(body: &[u8], switches: &ObservationSwitches) -> Option<Vec<u8>> {
    switches
        .downstream_log_body
        .then(|| redacted(body, switches.redact).into_owned())
}

/// The same field names core's observer treats as secret, so a downstream and
/// an upstream record of one request hide the same things. Compared with
/// dashes folded to underscores and case ignored, which is how a header name
/// and a JSON field spell the same idea.
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

/// `[name, value]` pairs in wire order, repeated headers included, which is
/// what the column is documented to hold.
fn headers_json(headers: &HeaderMap, redact: bool) -> Value {
    json!(
        headers
            .iter()
            .map(|(name, value)| (
                name.as_str(),
                if redact && sensitive(name.as_str()) {
                    REDACTED.to_owned()
                } else {
                    String::from_utf8_lossy(value.as_bytes()).into_owned()
                }
            ))
            .collect::<Vec<_>>()
    )
}

/// Mask sensitive parameters and keep everything else byte for byte: order and
/// repeats are part of what a captured query is for.
fn redact_query(query: &str) -> String {
    query
        .split('&')
        .map(|part| {
            let (key, _) = part.split_once('=').unwrap_or((part, ""));
            let decoded = form_urlencoded::parse(key.as_bytes())
                .next()
                .map(|(key, _)| key.into_owned())
                .unwrap_or_default();
            if sensitive(&decoded) {
                format!("{key}={REDACTED}")
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// A body with every value under a sensitive key replaced, at any depth.
///
/// Key-based rather than core's value-based scan, because the two sides know
/// different things: core knows the credential it is about to send and masks
/// those bytes wherever they appear, while a host knows the shape of what a
/// client sent and not the secrets inside it.
///
/// **A body that is not JSON is stored as received.** There is no key to match
/// on, and guessing at secrets in opaque bytes would corrupt the capture
/// without reliably hiding anything — which is one more reason
/// `enable_downstream_log_body` is off by default.
fn redacted(body: &[u8], redact: bool) -> Cow<'_, [u8]> {
    if !redact {
        return Cow::Borrowed(body);
    }
    let masked = serde_json::from_slice(body)
        .ok()
        .and_then(|mut value: Value| mask(&mut value).then_some(value))
        .and_then(|value| serde_json::to_vec(&value).ok());
    // Nothing matched: keep the original bytes rather than a re-serialization
    // of them, so an unredacted capture is exactly what was sent.
    masked.map_or(Cow::Borrowed(body), Cow::Owned)
}

/// Whether anything was masked.
fn mask(value: &mut Value) -> bool {
    let mut changed = false;
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if sensitive(key) {
                    *child = Value::String(REDACTED.to_owned());
                    changed = true;
                } else {
                    changed |= mask(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                changed |= mask(item);
            }
        }
        _ => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switches_mirror_the_settings_row_and_its_defaults() {
        let switches = ObservationSwitches::from_settings(None);
        assert_eq!(switches, ObservationSwitches::default());
        assert!(!switches.downstream_log && !switches.upstream_log && switches.redact);
        assert!(
            !switches.downstream_log_body,
            "bodies are opt-in on both sides"
        );
    }

    #[test]
    fn redaction_hides_the_same_names_in_a_header_a_query_and_a_body() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-live".parse().unwrap());
        headers.insert("x-trace", "keep".parse().unwrap());
        assert_eq!(
            headers_json(&headers, true),
            json!([["authorization", REDACTED], ["x-trace", "keep"]])
        );
        assert_eq!(
            redact_query("model=m1&api_key=sk-live&api_key=other"),
            format!("model=m1&api_key={REDACTED}&api_key={REDACTED}"),
            "every occurrence of a repeated parameter, and the order kept"
        );
        let body = br#"{"model":"m1","auth":{"access_token":"sk-live"}}"#;
        assert_eq!(
            serde_json::from_slice::<Value>(&redacted(body, true)).unwrap(),
            json!({"model": "m1", "auth": {"access_token": REDACTED}}),
            "nested values are reached too"
        );
        assert_eq!(&*redacted(body, false), body, "the switch turns it off");
    }

    #[test]
    fn a_body_with_nothing_to_hide_is_stored_byte_for_byte() {
        // Not a re-serialization: whitespace and key order survive.
        let body = b"{ \"b\": 1,\n  \"a\": 2 }";
        assert_eq!(&*redacted(body, true), body);
        let opaque = b"\x00\xff not json";
        assert_eq!(&*redacted(opaque, true), opaque);
    }

    #[test]
    fn a_large_body_is_redacted_without_losing_other_content() {
        let switches = ObservationSwitches {
            downstream_log_body: true,
            ..ObservationSwitches::default()
        };
        let secret = "s".repeat(128 * 1024);
        let body =
            json!({"token": secret, "model": "m1", "content": "a".repeat(128 * 1024)}).to_string();
        let stored = capture_body(body.as_bytes(), &switches).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&stored).unwrap()["content"],
            "a".repeat(128 * 1024)
        );
        assert!(
            !String::from_utf8_lossy(&stored).contains("ssss"),
            "the secret must not survive because the body was long"
        );
    }

    #[test]
    fn the_body_switch_is_read_before_anything_is_copied() {
        let switches = ObservationSwitches::default();
        assert!(!switches.downstream_log_body);
        assert_eq!(capture_body(b"a prompt", &switches), None);
    }

    #[test]
    fn a_state_reports_interruption_and_the_switch_apart() {
        use record::CaptureBodyState as S;
        assert_eq!(body_state(false, true), S::NotCaptured);
        assert_eq!(body_state(true, true), S::Complete);
        assert_eq!(
            body_state(true, false),
            S::Partial,
            "a stream that stopped early is partial even if nothing was cut"
        );
    }
}
