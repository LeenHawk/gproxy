//! One upstream exchange or WebSocket connection/turn.

pub use super::capture_record::{BodyFraming, CaptureBodyState, CaptureKind, CaptureState};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "upstream_records")]
pub struct Model {
    #[sea_orm(unique_key = "by_time")]
    pub started_at_ms: i64,
    #[sea_orm(primary_key, auto_increment = false, unique_key = "by_time")]
    pub id: String,
    /// Initiating request, for provenance only. Associations live in capture_links.
    #[sea_orm(indexed)]
    pub initiator_request_id: Option<String>,
    pub attempt_id: Option<String>,
    pub attempt_ordinal: Option<i32>,
    pub kind: CaptureKind,
    /// WsTurn -> same-side WsConnection. HTTP/connection records leave it unset.
    #[sea_orm(indexed)]
    pub session_id: Option<String>,
    /// Protocol-provided lane/turn correlation, when available; not a unique key.
    pub stream_key: Option<String>,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    #[sea_orm(indexed)]
    pub provider_id: Option<String>,
    #[sea_orm(indexed)]
    pub credential_id: Option<String>,
    /// Historical agent assignment pinned when this call/socket/turn began.
    /// Never rewritten after a handoff; distinct from the captured WS session_id.
    #[sea_orm(indexed)]
    pub agent_assignment_id: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,

    // HTTP request: method, URL/path, query, headers, body. WsConnection holds
    // its handshake here; WsTurn has messages, not another HTTP request.
    pub request_method: Option<String>,
    /// Downstream path or actual upstream URL, excluding the query below.
    #[sea_orm(column_type = "Text")]
    pub request_url: Option<String>,
    /// Raw query without '?', preserving order and repeated parameters.
    #[sea_orm(column_type = "Text")]
    pub request_query: Option<String>,
    /// JSON [name, value] string pairs, preserving repeated headers.
    pub request_headers: Option<Json>,
    /// Content-addressed header set; the nullable JSON column is for legacy reads.
    #[sea_orm(indexed)]
    pub request_headers_hash: Option<String>,
    /// Buffered bytes stored directly in the database, never in FileObject.
    /// Event-backed bodies leave this unset; empty bytes mean a captured empty body.
    pub request_body: Option<Vec<u8>>,
    #[sea_orm(default_value = "identity")]
    pub request_body_encoding: String,
    #[sea_orm(indexed)]
    pub request_body_id: Option<String>,
    #[sea_orm(default_value = "buffered")]
    pub request_framing: BodyFraming,
    #[sea_orm(default_value = "not_captured")]
    pub request_body_state: CaptureBodyState,

    // HTTP response: status, headers, body. Only a WS handshake has an HTTP
    // status; each subsequent WS turn must not be assigned status 101/200.
    pub response_status: Option<i32>,
    pub response_headers: Option<Json>,
    /// Content-addressed header set; the nullable JSON column is for legacy reads.
    #[sea_orm(indexed)]
    pub response_headers_hash: Option<String>,
    pub response_body: Option<Vec<u8>>,
    #[sea_orm(default_value = "identity")]
    pub response_body_encoding: String,
    #[sea_orm(default_value = "buffered")]
    pub response_framing: BodyFraming,
    #[sea_orm(default_value = "not_captured")]
    pub response_body_state: CaptureBodyState,

    pub client_ip: Option<String>,
    /// Upstream-native usage once per physical call/turn.
    /// Independent billable history remains in UsageRecord.
    pub metrics: Option<Json>,
    #[sea_orm(default_value = "in_progress")]
    pub state: CaptureState,
    #[sea_orm(column_type = "Text")]
    pub error: Option<String>,
    /// Channel-classified response reason, independent of capture/HTTP success.
    #[sea_orm(indexed)]
    pub reason: Option<String>,
    pub first_response_at_ms: Option<i64>,
    /// Exchange/turn termination, or socket closure for WsConnection.
    /// Indexed for retention, which deletes the oldest-ended rows first.
    #[sea_orm(indexed)]
    #[sea_orm(indexed)]
    pub ended_at_ms: Option<i64>,

    #[sea_orm(
        belongs_to,
        relation_enum = "RequestHeaderSet",
        from = "request_headers_hash",
        to = "hash",
        on_delete = "Restrict"
    )]
    pub request_header_set: BelongsTo<Option<super::header_set::Entity>>,
    #[sea_orm(
        belongs_to,
        relation_enum = "ResponseHeaderSet",
        from = "response_headers_hash",
        to = "hash",
        on_delete = "Restrict"
    )]
    pub response_header_set: BelongsTo<Option<super::header_set::Entity>>,
    #[sea_orm(
        self_ref,
        relation_enum = "Session",
        relation_reverse = "Turns",
        from = "session_id",
        to = "id",
        on_delete = "SetNull"
    )]
    pub session: BelongsTo<Option<Entity>>,
    #[sea_orm(self_ref, relation_enum = "Turns", relation_reverse = "Session")]
    pub turns: HasMany<Entity>,
    #[sea_orm(has_many, relation_enum = "Events", via_rel = "Capture")]
    pub events: HasMany<super::upstream_event::Entity>,
    #[sea_orm(has_many, relation_enum = "TurnEvents", via_rel = "Turn")]
    pub turn_events: HasMany<super::upstream_event::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
