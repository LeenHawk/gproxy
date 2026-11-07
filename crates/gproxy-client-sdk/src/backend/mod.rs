use crate::Result;

use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture,
};

/// The transport behind a [`Client`](crate::Client). [`Remote`] calls a GPROXY
/// instance over HTTP; `gproxy_sdk::ClientBackend` (in the `gproxy-sdk` crate,
/// feature `client-sdk`) drives an in-process engine. Applications that
/// already own a transport implement it themselves.
pub trait Backend {
    #[cfg(feature = "realtime")]
    fn connect(
        &self,
        _operation: OperationKey,
        _model: &str,
        _request: WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
        Box::pin(async {
            Err(crate::Error::unsupported(
                "this backend does not support WebSocket connections",
            ))
        })
    }
    fn send(
        &self,
        operation: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>>;
}

impl<T: Backend + ?Sized> Backend for Box<T> {
    #[cfg(feature = "realtime")]
    fn connect(
        &self,
        operation: OperationKey,
        model: &str,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'_, Result<gproxy_protocol::capability::UpstreamConnection>> {
        (**self).connect(operation, model, request)
    }
    fn send(
        &self,
        operation: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        (**self).send(operation, model, request)
    }
}
#[cfg(feature = "remote")]
mod remote;
#[cfg(feature = "remote")]
pub use remote::Remote;
