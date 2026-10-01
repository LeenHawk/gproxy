//! WebRTC call setup: Create realtime call.md. SDP answer and Location remain
//! raw HTTP body/headers; multipart session is a direct JSON session object.
use super::session::RealtimeSessionCreateRequest;
use crate::{
    WireRequest, WireResponse,
    connection::{HttpBody, MultipartPart},
};

#[derive(Debug, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CreateRealtimeCallMultipartForm {
    pub sdp: MultipartPart,
    pub session: Option<RealtimeSessionCreateRequest>,
}
pub type CreateRealtimeCallRequest = WireRequest<CreateRealtimeCallMultipartForm>;
pub type CreateRealtimeCallResponse = WireResponse<HttpBody>;
/// JSON value of the multipart session part, without an extra wrapper object.
pub type RealtimeCallSessionJson = RealtimeSessionCreateRequest;

/// Frameless WebRTC setup at `POST /v1/live`. Unlike Realtime, this session
/// has no required `type: realtime`. Source: samples/codex/codex-rs/codex-api/
/// src/endpoint/realtime_websocket/methods_frameless_bidi.rs::session_json.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveSessionCreateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<super::RealtimeAudioConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation: Option<LiveDelegation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_items: Option<Vec<LiveInitialMessage>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: crate::Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveDelegation {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_filler: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: crate::Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveInitialMessage {
    #[serde(rename = "type")]
    pub type_: String,
    pub role: String,
    pub content: Vec<LiveInitialText>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: crate::Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveInitialText {
    #[serde(rename = "type")]
    pub type_: String,
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: crate::Rest,
}

#[derive(Debug, gproxy_protocol_macros::WireBuilder, gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CreateLiveCallMultipartForm {
    pub sdp: MultipartPart,
    pub session: LiveSessionCreateRequest,
}
pub type CreateLiveCallRequest = WireRequest<CreateLiveCallMultipartForm>;
pub type CreateLiveCallResponse = WireResponse<HttpBody>;

/// HTTP/WS entry paths for the OpenAI realtime family. The host still owns
/// authentication, model routing and upgrading the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeRoute<'a> {
    CreateCall,
    CreateLiveCall,
    Connect,
    Live { call_id: Option<&'a str> },
}
impl<'a> RealtimeRoute<'a> {
    /// `/live` serves HTTP setup and a WebSocket; the method disambiguates it.
    pub fn from_request(method: &http::Method, path: &'a str) -> Option<Self> {
        let route = Self::from_path(path)?;
        match (method, route) {
            (&http::Method::POST, Self::Live { call_id: None }) => Some(Self::CreateLiveCall),
            (&http::Method::POST, Self::CreateCall) => Some(route),
            (&http::Method::GET, Self::Connect | Self::Live { .. }) => Some(route),
            _ => None,
        }
    }
    /// Match native paths, with or without the public `/v1` mount.
    /// Path call IDs remain percent-encoded so adapters can preserve them.
    pub fn from_path(path: &'a str) -> Option<Self> {
        let path = path.strip_prefix("/v1/").unwrap_or(path);
        let path = path.trim_start_matches('/').trim_end_matches('/');
        match path {
            "realtime/calls" => Some(Self::CreateCall),
            "realtime" => Some(Self::Connect),
            "live" => Some(Self::Live { call_id: None }),
            _ => path
                .strip_prefix("live/")
                .filter(|id| !id.is_empty() && !id.contains('/') && !matches!(*id, "." | ".."))
                .map(|id| Self::Live { call_id: Some(id) }),
        }
    }
    pub fn operation(self) -> crate::OperationKey {
        crate::OperationKey {
            operation: match self {
                Self::CreateCall | Self::CreateLiveCall => crate::Operation::CreateRealtimeCall,
                _ => crate::Operation::ConnectRealtime,
            },
            dialect: crate::Dialect::OpenAi,
        }
    }
}
