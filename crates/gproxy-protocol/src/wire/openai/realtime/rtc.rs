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
