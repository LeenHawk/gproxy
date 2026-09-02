use bytes::Bytes;
use serde_json::Value;

use crate::channel::PreparedRequest;
use crate::usage::NormalizedUsage;

mod meter;

pub use meter::RealtimeMeter;

pub struct SessionPrepareCtx<'a> {
    pub request_body: &'a Bytes,
    pub request_headers: &'a http::HeaderMap,
    pub response_headers: &'a http::HeaderMap,
    pub upstream_model: &'a str,
    pub secret: &'a Value,
}

pub struct PreparedSession {
    pub id: String,
    pub request: PreparedRequest,
    pub termination: PreparedRequest,
    pub meter: RealtimeMeter,
}

pub type SessionPreparer =
    for<'a> fn(SessionPrepareCtx<'a>) -> Result<PreparedSession, crate::channel::ChannelError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionUsageKind {
    Primary,
    Transcription,
}

pub struct SessionUsage {
    pub kind: SessionUsageKind,
    pub model: String,
    pub usage: NormalizedUsage,
}

pub enum SessionObservation {
    None,
    Usage(SessionUsage),
    Compromised { reason: String, resync: bool },
}
