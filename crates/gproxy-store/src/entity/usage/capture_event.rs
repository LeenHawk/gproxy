//! Ordered stream chunks or WebSocket messages, not separate billable calls.
//! HTTP chunks append to the owning exchange; all WS messages append to the
//! WsConnection. Optional turn_id identifies the business turn without copying
//! the message. The primary key preserves order across interleaved WS turns.
//! This records application messages/chunks, not TCP packets or WS fragmentation.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub capture_id: String,
    /// Host-assigned monotonic order across both directions of the HTTP exchange
    /// or entire WS connection, including control messages and concurrent turns.
    #[sea_orm(primary_key, auto_increment = false)]
    pub sequence: i64,
    /// WS business turn, when identifiable; None for HTTP/control/unassigned
    /// messages. The turn must reference capture_id as its session_id.
    #[sea_orm(indexed)]
    pub turn_id: Option<String>,
    pub direction: CaptureDirection,
    pub kind: CaptureEventKind,
    /// Captured bytes under logging redaction policy. HTTP byte chunks retain
    /// SSE/NDJSON/JSON-array delimiters; WS text/binary preserves message boundaries.
    pub payload: Vec<u8>,
    pub observed_at_ms: i64,
    #[sea_orm(
        belongs_to,
        relation_enum = "Capture",
        from = "capture_id",
        to = "id",
        on_delete = "Cascade"
    )]
    pub capture: BelongsTo<super::capture_record::Entity>,
    #[sea_orm(
        belongs_to,
        relation_enum = "Turn",
        from = "turn_id",
        to = "id",
        on_delete = "SetNull"
    )]
    pub turn: BelongsTo<Option<super::capture_record::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}

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
