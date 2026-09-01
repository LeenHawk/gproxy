//! Boundary types: what hosts hand the core and what they get back.
//!
//! The core speaks `http` crate types plus the types here. Hosts build a
//! [`RequestCtx`] from their native request and render an [`ExecOutcome`]
//! into their native response; nothing framework-specific crosses this line.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};

use crate::funnel::Settled;

/// Wire primitives — defined at the contract layer, re-exported here for
/// hosts: the surface hooks and the engine share one stream type.
pub use gproxy_channel_api::{ByteStream, TransportError};

/// One inbound request, normalized. Bodies are buffered `Bytes`: transforms
/// and failover retries need the request replayable, and the refcounted
/// buffer keeps clones free.
#[derive(Debug, Clone)]
pub struct RequestCtx {
    /// Host-assigned id; threads every log line, capture row, and usage row.
    pub request_id: String,
    /// Host-observed caller address. Direct embedders may not have one.
    pub client_ip: Option<std::net::IpAddr>,
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Bytes,
    /// The host accepted a websocket upgrade for this request.
    pub upgrade: bool,
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

/// Response body stream. Zero-copy passthrough is the default path; see
/// [`ByteStream`] in the channel contract for the wasm `Send` note.
pub enum ResponseBody {
    Full(Bytes),
    Stream(ByteStream),
    WebSocket(Box<dyn gproxy_channel_api::WsDuplex>),
}

impl std::fmt::Debug for ResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(bytes) => f.debug_tuple("Full").field(&bytes.len()).finish(),
            Self::Stream(_) => f.write_str("Stream"),
            Self::WebSocket(_) => f.write_str("WebSocket"),
        }
    }
}

/// How the upstream answer was classified — defined in the channel
/// contract (deciding what a response means is channel knowledge) and
/// re-exported here for hosts.
pub use gproxy_channel_api::Disposition;

/// What the core hands back. Fields are public to read and move out of;
/// the private [`Settled`] proof makes outside construction impossible.
#[derive(Debug)]
pub struct ExecOutcome {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ResponseBody,
    pub disposition: Disposition,
    pub(crate) _settled: Settled,
}
