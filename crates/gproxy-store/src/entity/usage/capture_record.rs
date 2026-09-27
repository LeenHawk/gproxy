//! Shared capture types and a read projection over the separate upstream/downstream tables.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub side: CaptureSide,
    pub started_at_ms: i64,
    pub id: String,
    pub initiator_request_id: Option<String>,
    pub attempt_id: Option<String>,
    pub attempt_ordinal: Option<i32>,
    pub kind: CaptureKind,
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
    pub request_headers: Option<Json>,
    pub request_body: Option<Vec<u8>>,
    pub request_framing: BodyFraming,
    pub request_body_state: CaptureBodyState,
    pub response_status: Option<i32>,
    pub response_headers: Option<Json>,
    pub response_body: Option<Vec<u8>>,
    pub response_framing: BodyFraming,
    pub response_body_state: CaptureBodyState,
    pub client_ip: Option<String>,
    pub metrics: Option<Json>,
    pub state: CaptureState,
    pub error: Option<String>,
    pub reason: Option<String>,
    pub first_response_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum CaptureSide {
    #[sea_orm(string_value = "downstream")]
    Downstream,
    #[sea_orm(string_value = "upstream")]
    Upstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum CaptureKind {
    #[sea_orm(string_value = "http")]
    Http,
    #[sea_orm(string_value = "ws_connection")]
    WsConnection,
    #[sea_orm(string_value = "ws_turn")]
    WsTurn,
}

/// Buffered uses the inline body column; all other forms append CaptureEvents.
/// Stream events preserve captured byte chunks, including framing delimiters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum BodyFraming {
    #[sea_orm(string_value = "buffered")]
    Buffered,
    #[sea_orm(string_value = "bytes")]
    Bytes,
    #[sea_orm(string_value = "sse")]
    Sse,
    #[sea_orm(string_value = "ndjson")]
    NdJson,
    #[sea_orm(string_value = "json_array")]
    JsonArray,
    #[sea_orm(string_value = "websocket")]
    WebSocket,
}

/// Capture completeness is independent of HTTP/business success.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum CaptureBodyState {
    #[sea_orm(string_value = "not_captured")]
    NotCaptured,
    #[sea_orm(string_value = "recording")]
    Recording,
    #[sea_orm(string_value = "complete")]
    Complete,
    #[sea_orm(string_value = "partial")]
    Partial,
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum CaptureState {
    #[sea_orm(string_value = "in_progress")]
    InProgress,
    #[sea_orm(string_value = "completed")]
    Completed,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "cancelled")]
    Cancelled,
}

impl From<super::upstream_record::Model> for Model {
    fn from(row: super::upstream_record::Model) -> Self {
        Self {
            side: CaptureSide::Upstream,
            started_at_ms: row.started_at_ms,
            id: row.id,
            initiator_request_id: row.initiator_request_id,
            attempt_id: row.attempt_id,
            attempt_ordinal: row.attempt_ordinal,
            kind: row.kind,
            session_id: row.session_id,
            stream_key: row.stream_key,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            provider_id: row.provider_id,
            credential_id: row.credential_id,
            agent_assignment_id: row.agent_assignment_id,
            model: row.model,
            operation: row.operation,
            request_method: row.request_method,
            request_url: row.request_url,
            request_query: row.request_query,
            request_headers: row.request_headers,
            request_body: row.request_body,
            request_framing: row.request_framing,
            request_body_state: row.request_body_state,
            response_status: row.response_status,
            response_headers: row.response_headers,
            response_body: row.response_body,
            response_framing: row.response_framing,
            response_body_state: row.response_body_state,
            client_ip: row.client_ip,
            metrics: row.metrics,
            state: row.state,
            error: row.error,
            reason: row.reason,
            first_response_at_ms: row.first_response_at_ms,
            ended_at_ms: row.ended_at_ms,
        }
    }
}

impl From<super::downstream_record::Model> for Model {
    fn from(row: super::downstream_record::Model) -> Self {
        Self {
            side: CaptureSide::Downstream,
            started_at_ms: row.started_at_ms,
            id: row.id,
            initiator_request_id: row.initiator_request_id,
            attempt_id: row.attempt_id,
            attempt_ordinal: row.attempt_ordinal,
            kind: row.kind,
            session_id: row.session_id,
            stream_key: row.stream_key,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            provider_id: row.provider_id,
            credential_id: row.credential_id,
            agent_assignment_id: row.agent_assignment_id,
            model: row.model,
            operation: row.operation,
            request_method: row.request_method,
            request_url: row.request_url,
            request_query: row.request_query,
            request_headers: row.request_headers,
            request_body: row.request_body,
            request_framing: row.request_framing,
            request_body_state: row.request_body_state,
            response_status: row.response_status,
            response_headers: row.response_headers,
            response_body: row.response_body,
            response_framing: row.response_framing,
            response_body_state: row.response_body_state,
            client_ip: row.client_ip,
            metrics: row.metrics,
            state: row.state,
            error: row.error,
            reason: row.reason,
            first_response_at_ms: row.first_response_at_ms,
            ended_at_ms: row.ended_at_ms,
        }
    }
}
