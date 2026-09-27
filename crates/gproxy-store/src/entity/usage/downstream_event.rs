//! Ordered stream chunks or WebSocket messages, not separate billable calls.
//! HTTP chunks append to the owning exchange; all WS messages append to the
//! WsConnection. Optional turn_id identifies the business turn without copying
//! the message. The primary key preserves order across interleaved WS turns.
//! This records application messages/chunks, not TCP packets or WS fragmentation.

pub use super::capture_event::{CaptureDirection, CaptureEventKind};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "downstream_events")]
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
    pub capture: BelongsTo<super::downstream_record::Entity>,
    #[sea_orm(
        belongs_to,
        relation_enum = "Turn",
        from = "turn_id",
        to = "id",
        on_delete = "SetNull"
    )]
    pub turn: BelongsTo<Option<super::downstream_record::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
