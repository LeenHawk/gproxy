//! One physical HTTP exchange, WebSocket connection, or WebSocket business turn.
//! Both request and response live here. Downstream/upstream edges are separate:
//! a shared upstream record must not be copied for each downstream consumer.
//! Downstream Http/WsTurn IDs are the request_id used by UsageRecord. Upstream
//! records have independent IDs and obtain their callers through CaptureLink.
//! Historical identity IDs deliberately have no configuration foreign keys.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_records")]
pub struct Model {
    /// The first three columns form the `by_side` key: an index over
    /// `(side, started_at_ms, id)`, which is how the log lists read one side,
    /// newest first with the id breaking ties, so a page is a range read
    /// instead of a sort of the whole side. Unique only because the entity
    /// macro can express a composite index no other way (the trailing primary
    /// key makes it so).
    #[sea_orm(unique_key = "by_side")]
    pub side: CaptureSide,
    #[sea_orm(indexed, unique_key = "by_side")]
    pub started_at_ms: i64,
    #[sea_orm(primary_key, auto_increment = false, unique_key = "by_side")]
    pub id: String,
    /// Initiating request/attempt, retained even when downstream logging is off.
    /// Provenance only: sharing is represented by CaptureLink, not these fields.
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
    /// Buffered bytes stored directly in the database, never in FileObject.
    /// Event-backed bodies leave this unset; empty bytes mean a captured empty body.
    pub request_body: Option<Vec<u8>>,
    #[sea_orm(default_value = "buffered")]
    pub request_framing: BodyFraming,
    #[sea_orm(default_value = "not_captured")]
    pub request_body_state: CaptureBodyState,

    // HTTP response: status, headers, body. Only a WS handshake has an HTTP
    // status; each subsequent WS turn must not be assigned status 101/200.
    pub response_status: Option<i32>,
    pub response_headers: Option<Json>,
    pub response_body: Option<Vec<u8>>,
    #[sea_orm(default_value = "buffered")]
    pub response_framing: BodyFraming,
    #[sea_orm(default_value = "not_captured")]
    pub response_body_state: CaptureBodyState,

    pub client_ip: Option<String>,
    /// Upstream-native usage once per physical call/turn. Not per event or edge.
    /// Downstream billed usage remains in UsageRecord; edges imply no allocation.
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
    pub ended_at_ms: Option<i64>,

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
    pub events: HasMany<super::capture_event::Entity>,
    #[sea_orm(has_many, relation_enum = "TurnEvents", via_rel = "Turn")]
    pub turn_events: HasMany<super::capture_event::Entity>,
    #[sea_orm(has_many, relation_enum = "UpstreamLinks", via_rel = "Downstream")]
    pub upstream_links: HasMany<super::capture_link::Entity>,
    #[sea_orm(has_many, relation_enum = "DownstreamLinks", via_rel = "Upstream")]
    pub downstream_links: HasMany<super::capture_link::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

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
