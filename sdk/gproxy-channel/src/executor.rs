//! Thin single-request executor (L1 "minimal client" entry point).
//!
//! [`execute_once`] runs a single upstream request against one credential
//! of one channel. It does NOT retry, rotate credentials, or track
//! cross-call health state — that is the job of the multi-channel
//! engine in `gproxy-engine`.
//!
//! This module is currently a stub. A follow-up commit will populate
//! `execute_once` / `execute_once_stream` with the logic extracted from
//! the engine's per-request pipeline (finalize → sanitize → rewrite →
//! prepare_request → send → classify → normalize).

use crate::channel::Channel;
use crate::request::PreparedRequest;
use crate::response::{UpstreamError, UpstreamResponse, UpstreamStreamingResponse};

/// Placeholder — full implementation in a follow-up commit.
#[allow(clippy::unused_async, unused_variables)]
pub async fn execute_once<C: Channel>(
    _channel: &C,
    _credential: &C::Credential,
    _settings: &C::Settings,
    _request: PreparedRequest,
    _http_client: &wreq::Client,
) -> Result<UpstreamResponse, UpstreamError> {
    Err(UpstreamError::Channel(
        "execute_once: not yet implemented (Step 2 stub)".to_string(),
    ))
}

/// Placeholder — full implementation in a follow-up commit.
#[allow(clippy::unused_async, unused_variables)]
pub async fn execute_once_stream<C: Channel>(
    _channel: &C,
    _credential: &C::Credential,
    _settings: &C::Settings,
    _request: PreparedRequest,
    _http_client: &wreq::Client,
) -> Result<UpstreamStreamingResponse, UpstreamError> {
    Err(UpstreamError::Channel(
        "execute_once_stream: not yet implemented (Step 2 stub)".to_string(),
    ))
}
