//! Responses WebSocket messages, separate from the HTTP upgrade handshake.
//!
//! Sources: the Responses WebSocket events reference and WebSocket mode guide.
//! Lane/prefill controls stay separate from HTTP DTOs. Steering has its own
//! acknowledgements, pending inputs, and automatic continuation lifecycle.
//! The transport/host owns the duplex connection and continuation orchestration.

use super::{generate::GenerateContentRequestBody, stream::StreamEvent};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
// Preserve the established by-value response.create constructor.
#[allow(clippy::large_enum_variant)]
pub enum ClientEvent {
    #[serde(rename = "response.inject")]
    ResponseInject(super::multi_agent::InjectRequest),
    #[serde(rename = "response.create")]
    ResponseCreate(GenerateContentRequestBody),
    #[serde(rename = "response.steer")]
    ResponseSteer(super::steering::SteerRequest),
    #[serde(rename = "response.interrupt")]
    ResponseInterrupt(InterruptRequest),
}

/// Generation and steering have distinct lifecycles on the same connection.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
// Match the by-value event DTOs used by native generation streams.
#[allow(clippy::large_enum_variant)]
pub enum ServerEvent {
    Interrupt(InterruptEvent),
    Injection(super::multi_agent::InjectionEvent),
    Steering(super::steering::SteeringEvent),
    Response(StreamEvent),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct InterruptRequest {
    pub response_id: String,
    pub mode: InterruptMode,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum InterruptMode {
    DiscardPartialItems,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(tag = "type")]
pub enum InterruptEvent {
    #[serde(rename = "response.interrupt.accepted")]
    Accepted {
        response_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.output_item.interrupted")]
    OutputItem {
        response_id: String,
        item_id: String,
        output_index: i64,
        sequence_number: i64,
    },
}
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
    pub event: ServerEvent,
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
