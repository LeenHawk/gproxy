use bytes::Bytes;
use gproxy_protocol::{OperationKey, StreamFraming};
use http::Request;
use serde_json::Value;

use crate::wire::ClientProfile;

/// Everything `prepare` may read. Borrowed views: preparation copies
/// nothing it does not rewrite.
#[derive(Clone, Copy)]
pub struct PrepareCtx<'a> {
    pub key: OperationKey,
    /// Caller-scoped conversation identity, stable across turns and retry targets.
    pub session_id: Option<&'a str>,
    pub stream: bool,
    pub method: &'a http::Method,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: &'a http::HeaderMap,
    pub body: &'a Bytes,
    /// Model id after alias/variant mapping — what the upstream receives.
    pub upstream_model: &'a str,
    pub provider_settings: &'a Value,
    /// Decrypted secret material in this channel's documented shape.
    pub secret: &'a Value,
}

/// The upstream request, ready to send.
pub struct PreparedRequest {
    pub request: Request<Bytes>,
    /// Actual upstream stream framing when it differs from the operation's
    /// protocol default (for example an explicit Gemini `alt=sse`).
    pub framing: Option<StreamFraming>,
    /// The transport must upgrade to a websocket instead of plain HTTP.
    pub websocket: bool,
    /// Native client fingerprint declared by the channel. The core carries it
    /// in request extensions for transports that can apply it.
    pub profile: Option<&'static ClientProfile>,
}

impl PreparedRequest {
    pub fn apply_profile(&mut self) {
        if let Some(profile) = self.profile {
            self.request.extensions_mut().insert(profile.clone());
        }
    }
}
