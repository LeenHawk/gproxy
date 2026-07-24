//! The output of [`Channel::prepare`](crate::channel::Channel::prepare).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;

use crate::http::client::{ClientError, UpstreamClient};

/// A channel-driven multi-step upstream exchange. The pipeline injects the
/// resolved `(proxy, emulation)` client; the closure performs whatever sequence
/// of calls it needs and returns the finished, buffered response. The closure
/// owns whatever else it needs (secret, inbound body) by `move`. Each call made
/// through the injected client is logged (§8-D) by the pipeline's capturing
/// wrapper, so the channel never resolves a proxy/client itself or persists
/// anything.
#[cfg(not(target_arch = "wasm32"))]
pub type CustomSend = Box<
    dyn FnOnce(
            Arc<dyn UpstreamClient>,
        )
            -> Pin<Box<dyn Future<Output = Result<http::Response<Bytes>, ClientError>> + Send>>
        + Send,
>;
/// wasm variant: the upstream future is `?Send` (see [`UpstreamClient`]).
#[cfg(target_arch = "wasm32")]
pub type CustomSend = Box<
    dyn FnOnce(
        Arc<dyn UpstreamClient>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<Bytes>, ClientError>>>>,
>;

/// A channel-driven multi-step exchange that returns a STREAMING body (native
/// only). Like [`CustomSend`] but yields `(status, headers, stream)` so a
/// long-running exchange streams the response incrementally instead of buffering
/// the whole thing.
#[cfg(not(target_arch = "wasm32"))]
pub type CustomStreamSend = Box<
    dyn FnOnce(
            Arc<dyn UpstreamClient>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            (
                                http::StatusCode,
                                http::HeaderMap,
                                crate::http::client::RespStream,
                            ),
                            ClientError,
                        >,
                    > + Send,
            >,
        > + Send,
>;

/// The output of [`Channel::prepare`]: either a single direct upstream request
/// (the common case — the pipeline sends it once), or a channel-driven
/// multi-step exchange ([`CustomSend`]).
///
/// Proxy and TLS-emulation are NOT carried here — they are per-credential /
/// global / channel-default concerns resolved by the executor
/// (see [`crate::channel::resolve`]), not the channel's to decide; the executor
/// injects the resolved client into a `Custom` closure.
// `Direct` (a full `http::Request`) is the hot path — every normal request. The
// size gap vs the boxed `Custom` closure is real, but boxing `Direct` to close
// it would add a heap allocation to EVERY request for the sake of the rare
// multi-step exchange; not worth it. The value is short-lived (one per attempt).
#[allow(clippy::large_enum_variant)]
pub enum PreparedRequest {
    /// Normal single send. `request.uri()` MUST be absolute (scheme + authority
    /// + path + query) — wreq cannot route a relative URI.
    Direct(http::Request<Bytes>),
    /// Channel-driven buffered multi-step exchange.
    Custom(CustomSend),
    /// Channel-driven multi-step exchange that streams its body incrementally
    /// Native only.
    #[cfg(not(target_arch = "wasm32"))]
    CustomStream(CustomStreamSend),
}

impl PreparedRequest {
    /// Wrap a built request for a normal single send.
    pub fn new(request: http::Request<Bytes>) -> Self {
        Self::Direct(request)
    }

    /// Wrap a channel-driven multi-step exchange closure.
    pub fn custom(send: CustomSend) -> Self {
        Self::Custom(send)
    }

    /// Wrap a streaming channel-driven multi-step exchange closure.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn custom_stream(send: CustomStreamSend) -> Self {
        Self::CustomStream(send)
    }

    /// Consume the direct request for the transport. Only valid on
    /// [`Direct`](PreparedRequest::Direct) — callers that never produce a
    /// `Custom` (the admin model-pull, tests) use this.
    pub fn into_http(self) -> http::Request<Bytes> {
        match self {
            Self::Direct(r) => r,
            _ => unreachable!("into_http called on a Custom-exchange PreparedRequest"),
        }
    }
}

impl std::fmt::Debug for PreparedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(r) => f.debug_tuple("Direct").field(r).finish(),
            Self::Custom(_) => f.write_str("Custom(<closure>)"),
            #[cfg(not(target_arch = "wasm32"))]
            Self::CustomStream(_) => f.write_str("CustomStream(<closure>)"),
        }
    }
}
