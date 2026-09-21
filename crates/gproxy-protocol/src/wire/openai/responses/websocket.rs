//! Responses WebSocket messages, separate from the HTTP upgrade handshake.
//!
//! Sources: `upstream_docs/openai/docs/Responses.md`, "Responses Client Event"
//! and "Responses Server Event". The client `response.create` object has the
//! older snapshot shares the HTTP request and 53 SSE event shapes. The current
//! official WebSocket mode guide additionally declares lane/prefill controls
//! and nested WS errors; the message envelopes below keep these WS-only fields
//! separate from HTTP DTOs. The operational adapter does not use HTTP stream
//! or background controls.
//! The transport adapter serializes these JSON messages into WebSocket text
//! messages; it remains responsible for the independent duplex connection.

use super::{generate::GenerateContentRequestBody, stream::StreamEvent};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ClientEvent {
    #[serde(rename = "response.create")]
    ResponseCreate(GenerateContentRequestBody),
}

pub type ServerEvent = StreamEvent;
pub type HandshakeRequest = crate::WireRequest<()>;
pub type HandshakeResponse = crate::WireResponse<()>;

/// WS-only controls from the current official WebSocket mode guide.
/// They never enter the HTTP generation request body.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct RequestMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate: Option<bool>,
    #[serde(flatten)]
    pub event: ClientEvent,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ServerMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    #[serde(flatten)]
    pub event: StreamEvent,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ErrorMessage {
    #[serde(rename = "type")]
    pub type_: ErrorMessageType,
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    pub error: ErrorDetail,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum ErrorMessageType {
    #[serde(rename = "error")]
    Error,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ErrorDetail {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}
