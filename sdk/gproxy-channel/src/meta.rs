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
    /// TTFB for the final attempt that produced this meta: time from
    /// sending the upstream HTTP request to receiving its response headers.
    pub initial_latency_ms: u64,
    /// Total latency for the final attempt: time from sending the upstream
    /// HTTP request to the body being fully read (buffered) or the stream
    /// being fully drained (streaming). Always `>= initial_latency_ms`.
    pub total_latency_ms: u64,
    pub credential_index: Option<usize>,
}
