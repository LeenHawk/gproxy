//! Responses WebSocket messages, separate from the HTTP upgrade handshake.
//!
//! Sources: `upstream_docs/openai/docs/Responses.md`, "Responses Client Event"
//! and "Responses Server Event". The client `response.create` object has the
//! same 31 fields as the HTTP generation body plus its `type` discriminator;
//! server messages use the same 53 payload shapes as the HTTP SSE event union.
//! The transport adapter serializes these JSON messages into WebSocket text
//! messages; it remains responsible for the independent duplex connection.

use super::{generate::GenerateContentRequestBody, stream::StreamEvent};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ClientEvent {
    #[serde(rename = "response.create")]
    ResponseCreate(GenerateContentRequestBody),
}

pub type ServerEvent = StreamEvent;
pub type HandshakeRequest = crate::WireRequest<()>;
pub type HandshakeResponse = crate::WireResponse<()>;
