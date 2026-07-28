//! Input contexts passed to channel hooks.

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

/// Declared upstream transport, retained for compatibility with existing
/// channel implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Http,
    Ws,
}

/// Per-call inputs used to build an upstream request.
pub struct PrepareCtx<'a> {
    pub secret: &'a Value,
    pub provider_settings: &'a Value,
    pub op: crate::protocol::OperationKey,
    pub stream: bool,
    pub upstream_model_id: &'a str,
    pub method: http::Method,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: &'a HeaderMap,
    pub body: Bytes,
}

/// Operation and settings available to request/response shaping hooks.
#[derive(Debug, Clone, Copy)]
pub struct ShapeCtx<'a> {
    pub op: crate::protocol::OperationKey,
    pub stream: bool,
    pub status: StatusCode,
    pub settings: &'a Value,
}

/// Inputs for a host-orchestrated credential refresh.
#[derive(Debug, Clone, Copy)]
pub struct RefreshCtx<'a> {
    pub secret: &'a Value,
    pub provider_settings: &'a Value,
}
