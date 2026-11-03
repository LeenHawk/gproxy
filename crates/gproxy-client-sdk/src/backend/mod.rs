use crate::Result;

use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture,
};

/// Two implementations share one caller API. This is also the injection point
/// for applications that already own a transport.
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
#[cfg(all(feature = "embedded", not(target_arch = "wasm32")))]
mod embedded;
#[cfg(all(feature = "embedded", not(target_arch = "wasm32")))]
pub use embedded::Embedded;
