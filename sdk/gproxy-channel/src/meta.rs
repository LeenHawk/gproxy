//! Upstream request metadata for logging and storage.

/// Metadata about the upstream request for logging/storage.
///
/// Emitted by the engine when it records the outcome of a request
/// (success, retry, failure) into the upstream-request log. Channels
/// may also populate this when they short-circuit the engine's normal
/// retry loop (e.g. Claude Code's credential dance).
#[derive(Debug, Clone)]
pub struct UpstreamRequestMeta {
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    /// Raw upstream response body, captured before any cross-protocol
    /// transform or stream aggregation. Populated only when the engine is
    /// built with `enable_upstream_log_body = true`; otherwise `None`.
    pub response_body: Option<Vec<u8>>,
    pub model: Option<String>,
    pub latency_ms: u64,
    pub credential_index: Option<usize>,
}
