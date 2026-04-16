//! Thin single-request executor — the L1 "minimal client" entry point.
//!
//! [`execute_once`] and [`execute_once_stream`] run a complete single
//! upstream request against ONE credential of ONE channel: finalize the
//! request, apply the channel's sanitize / rewrite rules, build the HTTP
//! request, send it, normalize the response body, and classify the
//! result. They do NOT retry, rotate credentials, or track cross-call
//! health state — that is the job of the multi-channel engine in
//! `gproxy-engine`.
//!
//! # Example (illustrative — using the OpenAI channel)
//!
//! ```rust,ignore
//! use gproxy_channel::channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings};
//! use gproxy_channel::routing::RouteKey;
//! use gproxy_channel::executor::execute_once;
//! use gproxy_channel::request::PreparedRequest;
//! use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
//!
//! # async fn demo() -> Result<(), gproxy_channel::response::UpstreamError> {
//! let channel = OpenAiChannel;
//! let settings = OpenAiSettings::default();
//! let credential = OpenAiCredential { api_key: std::env::var("OPENAI_API_KEY").unwrap() };
//! let http_client = wreq::Client::new();
//!
//! let request = PreparedRequest {
//!     method: http::Method::POST,
//!     route: RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion),
//!     model: Some("gpt-4o-mini".to_string()),
//!     body: br#"{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}"#.to_vec(),
//!     headers: http::HeaderMap::new(),
//! };
//!
//! let outcome = execute_once(&channel, &credential, &settings, &http_client, request).await?;
//! println!("status={} classification={:?}", outcome.response.status, outcome.classification);
//! # Ok(())
//! # }
//! ```

use crate::channel::{Channel, ChannelSettings};
use crate::http_client::{send_request, send_request_stream};
use crate::request::PreparedRequest;
use crate::response::{
    ResponseClassification, RetryableUpstreamResponse, UpstreamError, UpstreamResponse,
    UpstreamStreamingResponse,
};
use crate::utils::rewrite::{RewriteRule, apply_rewrite_rules};
use crate::utils::sanitize::{SanitizeRule, apply_sanitize_rules};

/// Outcome of one upstream attempt: the normalized upstream response
/// plus the classification assigned by the channel.
///
/// `classification` tells the caller whether the response is a success,
/// an auth failure, a rate limit, a transient error, or a permanent
/// error. The caller decides what to do on non-success — retry, surface
/// to the end user, etc.
#[derive(Debug)]
pub struct ExecuteOnceResult {
    pub response: UpstreamResponse,
    pub classification: ResponseClassification,
}

/// Outcome of one streaming upstream attempt.
///
/// - [`SendAttemptStreamOutcome::Streaming`] — 2xx response, body held
///   as an async byte stream. Cannot be classified since the body isn't
///   consumed yet; assume Success.
/// - [`SendAttemptStreamOutcome::Buffered`] — non-2xx response whose
///   body was fully buffered so it could be classified (exactly the
///   same shape as the non-streaming [`ExecuteOnceResult`]).
///
/// Not `Debug` because the `Streaming` variant holds an opaque async
/// stream handle.
pub enum SendAttemptStreamOutcome {
    /// Successful (2xx) streaming response whose body has not yet been
    /// consumed. The caller should forward the stream to the downstream
    /// client as-is.
    Streaming(UpstreamStreamingResponse),
    /// Non-success response whose body was fully buffered and classified
    /// so the caller can decide whether to retry or report the error.
    Buffered(ExecuteOnceResult),
}

/// Apply the channel's sanitize + rewrite rules to an outgoing request
/// body in a single JSON round-trip.
///
/// This is the **single in-tree invocation point** of
/// [`apply_sanitize_rules`] and [`apply_rewrite_rules`] — every other
/// consumer (the L1 executor pipeline, `gproxy-engine`'s per-attempt
/// sanitize stage, `gproxy-api`'s handler rewrite stage) calls this
/// helper instead of touching the raw pure functions directly.
///
/// Batching both rule kinds into one helper means we only
/// serialize / deserialize the request body once per request even when
/// both kinds of rules are configured, and it keeps the "what happens
/// on the outbound body" logic in exactly one place.
///
/// Non-JSON bodies (multipart uploads, raw binary streams, etc.) are
/// left untouched — the helper detects a parse failure and returns
/// early without mutating the body.
pub fn apply_outgoing_rules(
    request: &mut PreparedRequest,
    sanitize: &[SanitizeRule],
    rewrite: &[RewriteRule],
) {
    if sanitize.is_empty() && rewrite.is_empty() {
        return;
    }
    let Ok(mut body_json) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
        // Non-JSON body (e.g. multipart upload): leave as-is.
        return;
    };
    if !sanitize.is_empty() {
        apply_sanitize_rules(&mut body_json, request.route.protocol, sanitize);
    }
    if !rewrite.is_empty() {
        apply_rewrite_rules(
            &mut body_json,
            rewrite,
            request.model.as_deref(),
            request.route.operation,
            request.route.protocol,
        );
    }
    if let Ok(patched) = serde_json::to_vec(&body_json) {
        request.body = patched;
    }
}

/// Apply channel-owned pre-send transformations to a `PreparedRequest`.
///
/// Runs in order:
/// 1. `channel.finalize_request(settings, request)` — channel-specific
///    semantic normalization (protocol tweaks, default injection, etc.)
/// 2. [`apply_outgoing_rules`] — applies `ChannelSettings::sanitize_rules()`
///    and `ChannelSettings::rewrite_rules()` in one body pass.
///
/// External users typically don't need to call this directly — it is
/// called internally by [`execute_once`] and [`execute_once_stream`].
/// It is exposed as a public helper so that higher-level pipelines
/// (e.g. `gproxy-engine`'s retry loop) can run it once and then reuse
/// the prepared body for multiple credential attempts without redoing
/// the work each time.
pub fn prepare_for_send<C: Channel>(
    channel: &C,
    settings: &C::Settings,
    request: PreparedRequest,
) -> Result<PreparedRequest, UpstreamError> {
    let mut prepared = channel.finalize_request(settings, request)?;
    apply_outgoing_rules(
        &mut prepared,
        settings.sanitize_rules(),
        settings.rewrite_rules(),
    );
    Ok(prepared)
}

/// Send a single upstream request attempt with one credential.
///
/// Does NOT finalize / sanitize / rewrite — call [`prepare_for_send`]
/// first if your `PreparedRequest` has not been through those steps.
/// Does NOT retry or rotate credentials.
///
/// Pipeline:
/// 1. `channel.prepare_request(credential, settings, request)` →
///    `http::Request<Vec<u8>>` (adds auth headers, builds URL)
/// 2. HTTP send via `http_client` (uses `spoof_client` for channels
///    whose credential requires browser impersonation — falls back to
///    `http_client` when `spoof_client` is `None`)
/// 3. `channel.normalize_response(&request, body)` — channel-specific
///    body fixups
/// 4. `channel.classify_response(status, headers, body)` →
///    [`ResponseClassification`]
pub async fn send_attempt<C: Channel>(
    channel: &C,
    credential: &C::Credential,
    settings: &C::Settings,
    http_client: &wreq::Client,
    spoof_client: Option<&wreq::Client>,
    request: &PreparedRequest,
) -> Result<ExecuteOnceResult, UpstreamError> {
    let http_request = channel.prepare_request(credential, settings, request)?;

    let active_client = if channel.needs_spoof_client(credential) {
        spoof_client.unwrap_or(http_client)
    } else {
        http_client
    };

    let raw = send_request(active_client, http_request).await?;
    let normalized_body = channel.normalize_response(request, raw.body);
    let classification = channel.classify_response(raw.status, &raw.headers, &normalized_body);

    Ok(ExecuteOnceResult {
        response: UpstreamResponse {
            status: raw.status,
            headers: raw.headers,
            body: normalized_body,
            initial_latency_ms: raw.initial_latency_ms,
            total_latency_ms: raw.total_latency_ms,
        },
        classification,
    })
}

/// Streaming counterpart of [`send_attempt`]: same pipeline but keeps
/// 2xx upstream bodies as a byte stream. Non-2xx responses are fully
/// buffered so they can be classified.
pub async fn send_attempt_stream<C: Channel>(
    channel: &C,
    credential: &C::Credential,
    settings: &C::Settings,
    http_client: &wreq::Client,
    spoof_client: Option<&wreq::Client>,
    request: &PreparedRequest,
) -> Result<SendAttemptStreamOutcome, UpstreamError> {
    let http_request = channel.prepare_request(credential, settings, request)?;

    let active_client = if channel.needs_spoof_client(credential) {
        spoof_client.unwrap_or(http_client)
    } else {
        http_client
    };

    match send_request_stream(active_client, http_request).await? {
        RetryableUpstreamResponse::Streaming(stream) => {
            Ok(SendAttemptStreamOutcome::Streaming(stream))
        }
        RetryableUpstreamResponse::Buffered(buffered) => {
            let normalized_body = channel.normalize_response(request, buffered.body);
            let classification =
                channel.classify_response(buffered.status, &buffered.headers, &normalized_body);
            Ok(SendAttemptStreamOutcome::Buffered(ExecuteOnceResult {
                response: UpstreamResponse {
                    status: buffered.status,
                    headers: buffered.headers,
                    body: normalized_body,
                    initial_latency_ms: buffered.initial_latency_ms,
                    total_latency_ms: buffered.total_latency_ms,
                },
                classification,
            }))
        }
    }
}

/// Execute a single upstream request end-to-end against ONE credential.
///
/// Full pipeline: [`prepare_for_send`] (finalize + sanitize + rewrite)
/// followed by [`send_attempt`] (prepare_request + send + normalize +
/// classify). Returns an [`ExecuteOnceResult`] with the response and
/// its classification.
///
/// Use this when you want a typed single-provider client and you are
/// willing to handle credential selection / retry / health tracking
/// yourself. For a full multi-channel engine with automatic failover,
/// use `gproxy_engine::GproxyEngine` instead.
pub async fn execute_once<C: Channel>(
    channel: &C,
    credential: &C::Credential,
    settings: &C::Settings,
    http_client: &wreq::Client,
    request: PreparedRequest,
) -> Result<ExecuteOnceResult, UpstreamError> {
    let prepared = prepare_for_send(channel, settings, request)?;
    send_attempt(channel, credential, settings, http_client, None, &prepared).await
}

/// Streaming counterpart of [`execute_once`].
///
/// Returns [`SendAttemptStreamOutcome::Streaming`] for 2xx upstream
/// responses whose body is still unconsumed, or
/// [`SendAttemptStreamOutcome::Buffered`] for non-2xx responses whose
/// body was buffered and classified.
pub async fn execute_once_stream<C: Channel>(
    channel: &C,
    credential: &C::Credential,
    settings: &C::Settings,
    http_client: &wreq::Client,
    request: PreparedRequest,
) -> Result<SendAttemptStreamOutcome, UpstreamError> {
    let prepared = prepare_for_send(channel, settings, request)?;
    send_attempt_stream(channel, credential, settings, http_client, None, &prepared).await
}
