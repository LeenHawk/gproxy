//! The transport extension point. Implementations receive an already prepared
//! absolute request and never choose credentials or perform protocol conversion.

use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityFuture, UpstreamConnection},
};

use crate::channel::ChannelError;

/// Native clients must be shareable; WASM transports may own local JS handles.
#[cfg(not(target_arch = "wasm32"))]
pub trait ClientBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> ClientBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait ClientBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> ClientBounds for T {}

/// A host-selected HTTP client with optional WebSocket support, independent of
/// reqwest/wreq/Fetch.
///
/// Non-2xx responses remain complete responses. Implementations preserve streaming
/// bodies and enforce host-configured deadlines and transfer limits. They must
/// not implicitly retry a consumed body or forward authentication to a different
/// origin on redirect. Body-transfer errors stay on the returned body stream.
pub trait OutboundClient: ClientBounds {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, ChannelError>>;

    /// Optional duplex transport. Existing HTTP-only clients need no new code.
    fn connect<'a>(
        &'a self,
        _request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, ChannelError>> {
        Box::pin(async { Err(ChannelError::WebSocketUnavailable) })
    }
}
