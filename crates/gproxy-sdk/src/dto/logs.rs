//! Captured exchanges as a console reads them: the downstream request list,
//! and one request's whole tree of upstream attempts and stream events.
//!
//! Bodies carry all stored bytes. The capture state distinguishes absent,
//! interrupted and complete bodies; reading a log does not trim its payload.
//!
//! Nothing here redacts. Core's observer applied the logging redaction policy
//! when it wrote the rows — headers, query parameters and secret fragments in
//! the stream — so what is stored is already what may be shown. A host that
//! wants stricter redaction must change the policy, not filter on read: a
//! second pass here could not recover what was already written in the clear.

use base64::Engine;
use sea_orm::ActiveEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use gproxy_store::entity::usage::capture_record;

use super::UsageRecordDto;

/// How a body's bytes are spelled in `content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub enum LogBodyEncoding {
    /// Valid UTF-8.
    Utf8,
    /// Standard base64 with padding, for bytes that are not text.
    Base64,
}

/// One captured body, or the reason there is none.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct LogBodyDto {
    /// The record's own `body_state`: `not_captured`, `recording`,
    /// `complete`, `partial` or `failed`. `not_captured` with zero bytes is a
    /// body nobody stored; `complete` with zero bytes is a body that was
    /// genuinely empty.
    pub state: String,
    pub encoding: LogBodyEncoding,
    /// Compatibility field: false, because reads return the complete stored payload.
    pub truncated: bool,
    /// Bytes stored on the row, not the length of `content`.
    pub bytes: u64,
    pub content: String,
}

impl LogBodyDto {
    /// A body that was never stored. Distinct from an empty one: `content` is
    /// empty either way, and only `state` says which happened.
    pub(crate) fn absent(state: String) -> Self {
        Self {
            state,
            encoding: LogBodyEncoding::Utf8,
            truncated: false,
            bytes: 0,
            content: String::new(),
        }
    }

    /// Return all bytes as UTF-8 when valid, or losslessly encode binary as base64.
    pub(crate) fn new(state: String, bytes: &[u8]) -> Self {
        let (encoding, content) = match std::str::from_utf8(bytes) {
            Ok(text) => (LogBodyEncoding::Utf8, text.to_owned()),
            Err(_) => (
                LogBodyEncoding::Base64,
                base64::engine::general_purpose::STANDARD.encode(bytes),
            ),
        };
        Self {
            state,
            encoding,
            content,
            bytes: bytes.len() as u64,
            truncated: false,
        }
    }
}

/// One downstream request or physical upstream exchange, as a list row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct LogEntryDto {
    /// Capture ID. For downstream rows this is also the request/usage ID;
    /// upstream rows have their own physical exchange ID.
    pub request_id: String,
    /// `http`, `ws_connection` or `ws_turn`.
    pub kind: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub provider_id: Option<String>,
    pub credential_id: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,
    pub request_method: Option<String>,
    pub request_url: Option<String>,
    pub response_status: Option<i32>,
    /// `in_progress`, `completed`, `failed` or `cancelled`. Capture
    /// completeness, not the upstream's verdict.
    pub state: String,
    pub error: Option<String>,
    pub reason: Option<String>,
    pub client_ip: Option<String>,
    pub started_at_ms: i64,
    pub first_response_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
}

impl From<capture_record::Model> for LogEntryDto {
    fn from(row: capture_record::Model) -> Self {
        Self {
            request_id: row.id,
            kind: row.kind.to_value(),
            session_id: row.session_id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            provider_id: row.provider_id,
            credential_id: row.credential_id,
            model: row.model,
            operation: row.operation,
            request_method: row.request_method,
            request_url: row.request_url,
            response_status: row.response_status,
            state: row.state.to_value(),
            error: row.error,
            reason: row.reason,
            client_ip: row.client_ip,
            started_at_ms: row.started_at_ms,
            first_response_at_ms: row.first_response_at_ms,
            ended_at_ms: row.ended_at_ms,
        }
    }
}

/// One page of the request list, plus where the next one starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct LogPageDto {
    pub items: Vec<LogEntryDto>,
    /// The `started_at_ms` of the last item, or None at the end of the list.
    /// Hand both cursor halves back unchanged to get the next page.
    pub next_cursor: Option<i64>,
    /// The id of the same item. Two requests can share a millisecond, and a
    /// timestamp-only cursor would either repeat them forever or skip them;
    /// the pair is unique because ids are.
    pub next_cursor_id: Option<String>,
}

/// One captured exchange in full, downstream or upstream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CaptureRecordDto {
    pub id: String,
    pub initiator_request_id: Option<String>,
    pub attempt_id: Option<String>,
    pub attempt_ordinal: Option<i32>,
    /// `downstream` or `upstream`.
    pub side: String,
    /// `http`, `ws_connection` or `ws_turn`.
    pub kind: String,
    pub session_id: Option<String>,
    pub stream_key: Option<String>,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub provider_id: Option<String>,
    pub credential_id: Option<String>,
    pub agent_assignment_id: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,
    pub request_method: Option<String>,
    pub request_url: Option<String>,
    pub request_query: Option<String>,
    /// `[[name, value], …]`, repeated headers preserved.
    #[cfg_attr(feature = "ts", ts(type = "[string, string][] | null"))]
    pub request_headers: Option<Value>,
    pub request_body: LogBodyDto,
    /// `buffered`, `bytes`, `sse`, `ndjson`, `json_array` or `websocket`.
    /// Anything but `buffered` means the body is in the events, not the row.
    pub request_framing: String,
    pub response_status: Option<i32>,
    #[cfg_attr(feature = "ts", ts(type = "[string, string][] | null"))]
    pub response_headers: Option<Value>,
    pub response_body: LogBodyDto,
    pub response_framing: String,
    pub client_ip: Option<String>,
    /// Upstream-native usage for this one exchange, when the channel reported
    /// any. Billed usage is on the usage record, not here.
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub metrics: Option<Value>,
    pub state: String,
    pub error: Option<String>,
    pub reason: Option<String>,
    pub started_at_ms: i64,
    pub first_response_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
}

/// One stream chunk or websocket message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CaptureEventDto {
    pub capture_id: String,
    /// Monotonic across both directions of the exchange or connection.
    pub sequence: i64,
    pub turn_id: Option<String>,
    /// `request` (client to gateway, gateway to provider) or `response`.
    pub direction: String,
    /// `bytes`, `ws_text`, `ws_binary`, `ws_ping`, `ws_pong` or `ws_close`.
    pub kind: String,
    pub payload: LogBodyDto,
    pub observed_at_ms: i64,
}

/// One request and everything captured under it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct LogDetailDto {
    pub downstream: CaptureRecordDto,
    /// Every upstream attempt reached through `capture_links`, in link order.
    /// A retry is another entry here, not another downstream record.
    pub upstream: Vec<CaptureRecordDto>,
    /// The events of the downstream record and of every upstream one,
    /// ordered by capture and then by sequence.
    pub events: Vec<CaptureEventDto>,
    /// True when the event cap cut the list short.
    pub events_truncated: bool,
    /// The settled usage, when this request produced any.
    pub usage: Option<UsageRecordDto>,
}

/// What a request listing filters on. Timestamps bound `started_at_ms`:
/// `from_ms` is inclusive, `to_ms` exclusive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct LogQuery {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub provider_id: Option<String>,
    pub credential_id: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,
    /// Exact HTTP response status.
    pub status: Option<i32>,
    /// Exact normalized upstream reason (for example `refusal`).
    pub reason: Option<String>,
    /// One request by id, which for a downstream record is its own id.
    pub request_id: Option<String>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<i64>,
    /// `next_cursor_id` of the previous page. Without it a cursor is only a
    /// timestamp, and records sharing a millisecond would repeat.
    pub cursor_id: Option<String>,
    /// Clamped to 1..=500; absent means 50.
    pub limit: Option<u64>,
}

/// One physical capture, readable even when the downstream log is disabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CaptureDetailDto {
    pub record: CaptureRecordDto,
    pub events: Vec<CaptureEventDto>,
    pub events_truncated: bool,
}
