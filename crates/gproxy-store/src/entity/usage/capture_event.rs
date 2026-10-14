//! Shared event types and a read projection; events have side-specific owner foreign keys.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub capture_id: String,
    pub sequence: i64,
    pub turn_id: Option<String>,
    pub direction: CaptureDirection,
    pub kind: CaptureEventKind,
    pub payload: Vec<u8>,
    pub encoding: String,
    pub chunk_offsets: Option<Vec<u8>>,
    pub body_id: Option<String>,
    pub observed_at_ms: i64,
}

/// Request means client -> gateway downstream, gateway -> provider upstream.
/// Response is the reverse direction, including WS control messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum CaptureDirection {
    #[sea_orm(string_value = "request")]
    Request,
    #[sea_orm(string_value = "response")]
    Response,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum CaptureEventKind {
    #[sea_orm(string_value = "bytes")]
    Bytes,
    #[sea_orm(string_value = "ws_text")]
    WsText,
    #[sea_orm(string_value = "ws_binary")]
    WsBinary,
    #[sea_orm(string_value = "ws_ping")]
    WsPing,
    #[sea_orm(string_value = "ws_pong")]
    WsPong,
    /// Payload contains the close code and reason, when present.
    #[sea_orm(string_value = "ws_close")]
    WsClose,
}

impl From<super::upstream_event::Model> for Model {
    fn from(row: super::upstream_event::Model) -> Self {
        Self {
            capture_id: row.capture_id,
            sequence: row.sequence,
            turn_id: row.turn_id,
            direction: row.direction,
            kind: row.kind,
            payload: row.payload,
            encoding: row.encoding,
            chunk_offsets: row.chunk_offsets,
            body_id: row.body_id,
            observed_at_ms: row.observed_at_ms,
        }
    }
}

impl From<super::downstream_event::Model> for Model {
    fn from(row: super::downstream_event::Model) -> Self {
        Self {
            capture_id: row.capture_id,
            sequence: row.sequence,
            turn_id: row.turn_id,
            direction: row.direction,
            kind: row.kind,
            payload: row.payload,
            encoding: row.encoding,
            chunk_offsets: row.chunk_offsets,
            body_id: row.body_id,
            observed_at_ms: row.observed_at_ms,
        }
    }
}
