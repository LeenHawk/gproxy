//! Boundary types: what hosts hand the core and what they get back.
//!
//! The core speaks `http` crate types plus the types here. Hosts build a
//! [`RequestCtx`] from their native request and render an [`ExecOutcome`]
//! into their native response; nothing framework-specific crosses this line.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};

use crate::error::TransportError;

/// One inbound request, normalized. Bodies are buffered `Bytes`: transforms
/// and failover retries need the request replayable, and the refcounted
/// buffer keeps clones free.
#[derive(Debug, Clone)]
pub struct RequestCtx {
    /// Host-assigned id; threads every log line, capture row, and usage row.
    pub request_id: String,
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub mode: RoutingMode,
}

/// How the request addresses a backend (v2 semantics, kept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingMode {
    /// `/v1/...` — the model name in the body resolves through routes.
    Aggregated,
    /// `/{namespace}/v1/...` where the namespace is a route namespace.
    Namespace { namespace: String },
    /// `/{provider}/v1/...` — bypass routing, hit the named provider.
    Scoped { provider: String },
    /// A name not yet resolved to a namespace or provider; the resolve
    /// stage settles it from the control plane.
    Named { name: String },
}

/// Response body stream. Zero-copy passthrough is the default path: frames
/// flow as refcounted `Bytes` and are only re-encoded when a transform must
/// rewrite them.
///
/// The `Send` split is the one language-level tax the core carries for the
/// wasm target (single-threaded executors; futures there are not `Send`).
#[cfg(not(target_arch = "wasm32"))]
pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, TransportError>> + Send>>;
#[cfg(target_arch = "wasm32")]
pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, TransportError>>>>;

pub enum ResponseBody {
    Full(Bytes),
    Stream(ByteStream),
}

impl std::fmt::Debug for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(bytes) => f.debug_tuple("Full").field(&bytes.len()).finish(),
            Self::Stream(_) => f.write_str("Stream"),
        }
    }
}

/// How the upstream answer was classified — defined in the channel
/// contract (deciding what a response means is channel knowledge) and
/// re-exported here for hosts.
pub use gproxy_channel_api::Disposition;

/// Proof of settlement. Constructible only inside the funnel module —
/// every [`ExecOutcome`] carries one, so no code path can produce a
/// response that skipped settle/capture/telemetry. (v2's Codex bypass ran
/// unmetered for months precisely because nothing enforced this.)
#[derive(Debug)]
pub struct Settled(pub(crate) ());

/// What the core hands back. Fields are public to read and move out of;
/// the private [`Settled`] proof makes outside construction impossible.
#[derive(Debug)]
pub struct ExecOutcome {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ResponseBody,
    pub disposition: Disposition,
    #[expect(dead_code, reason = "proof token; read once the funnel lands")]
    pub(crate) settled: Settled,
}
