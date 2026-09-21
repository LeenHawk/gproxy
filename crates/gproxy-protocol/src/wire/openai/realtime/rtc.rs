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

/// HTTP/WS entry paths for the OpenAI realtime family. The host still owns
/// authentication, model routing and upgrading the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeRoute<'a> {
    CreateCall,
    Connect,
    Live { call_id: Option<&'a str> },
}
impl<'a> RealtimeRoute<'a> {
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
                Self::CreateCall => crate::Operation::CreateRealtimeCall,
                _ => crate::Operation::ConnectRealtime,
            },
            dialect: crate::Dialect::OpenAi,
        }
    }
}
