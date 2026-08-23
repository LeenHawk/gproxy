//! The channel trait and its request/response views.

use bytes::Bytes;
use gproxy_protocol::{OperationKey, StreamFraming};
use serde_json::Value;

use crate::BoxFuture;
use crate::disposition::Disposition;
use crate::surface::{SurfaceRequest, SurfaceTable};
use crate::usage::NormalizedUsage;
use crate::wire::ClientProfile;

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

/// One declared route through a channel: the client's wire shape and the
/// native wire shape the channel receives after any transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelSupport {
    pub source: OperationKey,
    pub target: OperationKey,
}

impl ChannelSupport {
    pub const fn passthrough(key: OperationKey) -> Self {
        Self {
            source: key,
            target: key,
        }
    }

    pub const fn transform(source: OperationKey, target: OperationKey) -> Self {
        Self { source, target }
    }
}

/// Identity and capability card. `supports` is the channel's declared route
/// table — the engine consults it before routing, and the console renders it
/// from the runtime catalog (no hand-maintained frontend copy).
#[derive(Debug)]
pub struct ChannelDescriptor {
    /// Stable id: `"openai"`, `"claudecode"`, `"codex"`.
    pub id: &'static str,
    pub display_name: &'static str,
    pub supports: &'static [ChannelSupport],
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
            self.request.extensions_mut().insert(*profile);
        }
    }
}

/// What classification may read. For streaming responses the body is
/// whatever error page arrived before streaming began, or empty.
pub struct ResponseView<'a> {
    pub status: http::StatusCode,
    pub headers: &'a http::HeaderMap,
    pub body: &'a [u8],
}

/// Context for constructing a per-response stream decoder. Usage observers
/// may need request parameters (audio format) and response metadata while
/// still returning an owned state machine.
pub struct StreamCtx<'a> {
    pub key: OperationKey,
    pub framing: StreamFraming,
    pub request_body: &'a Bytes,
    pub response_headers: &'a http::HeaderMap,
}

/// The complete buffered exchange visible to usage extraction.
pub struct UsageCtx<'a> {
    pub key: OperationKey,
    pub request_body: &'a Bytes,
    pub response_headers: &'a http::HeaderMap,
    pub response_body: &'a [u8],
}

/// Raw buffered upstream response visible to channel-private normalization
/// before any protocol-pair conversion. Capture and usage still consume the
/// unshaped bytes.
pub struct ResponseShapeCtx<'a> {
    pub key: OperationKey,
    pub status: http::StatusCode,
    pub headers: &'a http::HeaderMap,
    pub body: &'a Bytes,
}

/// One decoded stream frame, zero-copy where the wire allows.
#[derive(Debug)]
pub struct Frame(pub Bytes);

/// What a finished stream reports.
#[derive(Debug, Default)]
pub struct StreamTail {
    /// Frames completed only when the decoder observed EOF, such as an SSE
    /// event whose final blank-line delimiter was omitted.
    pub frames: Vec<Frame>,
    pub usage: Option<NormalizedUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEnd {
    Complete,
    Interrupted,
}

/// Stateful per-response stream decoder (SSE, AWS event-stream, ...).
/// A pure state machine: owned chunks in, frames out, tail at the end. Owning
/// the chunk lets an observe-only decoder relay it as a [`Frame`] without a
/// copy while still collecting usage state.
pub trait StreamDecoder: Send {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError>;
    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError>;
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
    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        let _ = ctx;
        None
    }

    /// Pull usage out of a buffered response body.
    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage>;

    /// Normalize a channel-private buffered envelope into its declared native
    /// target wire before the pairwise outward transform.
    fn shape_response(&self, ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
        Ok(ctx.body.clone())
    }

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

    /// Prepare a provider control-plane request declared by a surface entry.
    /// These paths have no [`OperationKey`], so they cannot use
    /// [`Channel::prepare`].
    fn prepare_surface(
        &self,
        request: &SurfaceRequest,
        websocket: bool,
        provider_settings: &Value,
        secret: &Value,
    ) -> Result<PreparedRequest, ChannelError> {
        let _ = (request, websocket, provider_settings, secret);
        Err(ChannelError::Prepare(
            "channel does not prepare surface requests".into(),
        ))
    }

    /// The service-surface table this channel brings (emulated vendor
    /// control-plane endpoints). Upstream path knowledge stays here — v2
    /// kept the `/wham/...` map in the HTTP layer and paid for it twice.
    fn surfaces(&self) -> SurfaceTable {
        SurfaceTable(&[])
    }
}
