//! The channel trait and its request/response views.

use bytes::Bytes;
use gproxy_protocol::OperationKey;
use serde_json::Value;

use crate::BoxFuture;
use crate::disposition::Disposition;
use crate::surface::SurfaceTable;
use crate::usage::NormalizedUsage;

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("credential secret malformed: {0}")]
    Secret(String),
    #[error("request preparation failed: {0}")]
    Prepare(String),
    #[error("refresh failed: {0}")]
    Refresh(String),
    #[error("decode failed: {0}")]
    Decode(String),
}

/// Identity and capability card. `supports` is the channel's declared
/// operation table — the engine consults it before routing, the console
/// renders it from the runtime catalog (no hand-maintained frontend copy).
#[derive(Debug)]
pub struct ChannelDescriptor {
    /// Stable id: `"openai"`, `"claudecode"`, `"codex"`.
    pub id: &'static str,
    pub display_name: &'static str,
    pub supports: &'static [OperationKey],
}

/// Everything `prepare` may read. Borrowed views: preparation copies
/// nothing it does not rewrite.
pub struct PrepareCtx<'a> {
    pub key: OperationKey,
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
    pub request: http::Request<Bytes>,
    /// The transport must upgrade to a websocket instead of plain HTTP.
    pub websocket: bool,
}

/// What classification may read. For streaming responses the body is
/// whatever error page arrived before streaming began, or empty.
pub struct ResponseView<'a> {
    pub status: http::StatusCode,
    pub headers: &'a http::HeaderMap,
    pub body: &'a [u8],
}

/// One decoded stream frame, zero-copy where the wire allows.
#[derive(Debug)]
pub struct Frame(pub Bytes);

/// What a finished stream reports.
#[derive(Debug, Default)]
pub struct StreamTail {
    pub usage: Option<NormalizedUsage>,
}

/// Stateful per-response stream decoder (SSE, AWS event-stream, ...).
/// A pure state machine: chunks in, frames out, tail at the end.
pub trait StreamDecoder: Send {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, ChannelError>;
    fn finish(&mut self) -> StreamTail;
}

/// Minimal buffered HTTP the engine lends to `refresh` — refresh calls are
/// small JSON exchanges; no streaming, no zero-copy concern.
pub trait SimpleHttp {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>>;
}

/// The contract. Synchronous and object-safe on purpose: adapters are pure
/// logic; I/O and state live in the engine and the host.
pub trait Channel: Send + Sync {
    fn descriptor(&self) -> &'static ChannelDescriptor;

    /// Build the upstream request: URL, auth injection, header allow-list,
    /// body shaping. Must not perform I/O.
    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError>;

    /// Classify one upstream answer for failover and health.
    fn classify(&self, response: ResponseView<'_>) -> Disposition;

    /// A decoder when this operation's response streams in a shape the
    /// engine cannot treat as opaque bytes; `None` = pass through.
    fn stream_decoder(&self, key: OperationKey) -> Option<Box<dyn StreamDecoder>> {
        let _ = key;
        None
    }

    /// Pull usage out of a buffered response body.
    fn extract_usage(&self, key: OperationKey, body: &[u8]) -> Option<NormalizedUsage>;

    /// Unix time after which the secret should be refreshed proactively;
    /// `None` = this channel's credentials never refresh.
    fn refresh_due(&self, secret: &Value) -> Option<i64> {
        let _ = secret;
        None
    }

    /// Refresh the secret. Returns the full replacement secret; the engine
    /// persists it through the host's version-guarded `CredentialStore`.
    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, ChannelError>>> {
        let _ = (secret, http);
        None
    }

    /// The service-surface table this channel brings (emulated vendor
    /// control-plane endpoints). Upstream path knowledge stays here — v2
    /// kept the `/wham/...` map in the HTTP layer and paid for it twice.
    fn surfaces(&self) -> SurfaceTable {
        SurfaceTable(&[])
    }
}
