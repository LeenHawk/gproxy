use std::pin::Pin;

use bytes::Bytes;
use futures_util::Stream;

/// Upstream response from a channel.
#[derive(Debug)]
pub struct UpstreamResponse {
    pub status: u16,
    pub headers: http::HeaderMap,
    pub body: Vec<u8>,
    /// Final upstream URL after any client-side redirect handling.
    pub url: String,
    /// Time from `send_request` entry until `client.execute().await` returned
    /// with response headers. Represents upstream TTFB for this attempt only.
    pub initial_latency_ms: u64,
    /// Time from `send_request` entry until the body was fully read.
    /// Equal to or greater than `initial_latency_ms`.
    pub total_latency_ms: u64,
}

pub type UpstreamBodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, UpstreamError>> + Send>>;

/// Upstream response whose body should be forwarded incrementally.
pub struct UpstreamStreamingResponse {
    pub status: u16,
    pub headers: http::HeaderMap,
    pub body: UpstreamBodyStream,
    /// Final upstream URL after any client-side redirect handling.
    pub url: String,
    /// TTFB, measured identically to `UpstreamResponse::initial_latency_ms`.
    pub initial_latency_ms: u64,
    /// Base instant for computing total latency. The stream consumer calls
    /// `stream_start.elapsed()` after draining the body. Callers that
    /// reconstruct this struct from an already-buffered response (see
    /// `retry::wrap_buffered`) back-date this instant so `elapsed()` still
    /// yields the correct upstream total.
    pub stream_start: std::time::Instant,
}

/// Transport-level response used by retry logic.
pub enum RetryableUpstreamResponse {
    Buffered(UpstreamResponse),
    Streaming(UpstreamStreamingResponse),
}

/// Classification of an upstream response for retry decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseClassification {
    /// 2xx — request succeeded.
    Success,
    /// 401/403 — credential permanently invalid.
    AuthDead,
    /// 429 — rate limited, retry with another credential.
    RateLimited { retry_after_ms: Option<u64> },
    /// 5xx transient — server error, worth retrying.
    TransientError,
    /// Other error — not worth retrying.
    PermanentError,
}

/// Captured metadata from an upstream attempt that did NOT succeed.
///
/// Emitted by the retry loop when it gives up without a usable response
/// (all credentials exhausted, all auth dead, transient errors etc.) so
/// the caller can persist the real upstream URL, request headers/body,
/// response status, response headers, and response body in the upstream
/// request log instead of writing a near-empty placeholder row.
#[derive(Debug, Default, Clone)]
pub struct FailedUpstreamAttempt {
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<Vec<u8>>,
    pub credential_index: Option<usize>,
}

/// Error from upstream channel execution.
#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("all credentials exhausted")]
    AllCredentialsExhausted,
    #[error("no eligible credentials")]
    NoEligibleCredentials,
    #[error("request build error: {0}")]
    RequestBuild(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("channel error: {0}")]
    Channel(String),
}

impl From<gproxy_protocol::transform::TransformError> for UpstreamError {
    fn from(err: gproxy_protocol::transform::TransformError) -> Self {
        UpstreamError::Channel(err.to_string())
    }
}
