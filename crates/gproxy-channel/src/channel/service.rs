//! Vendor CLI services without an OperationKey: profile, thread usage, bootstrap,
//! OAuth files/skills and remote-control endpoints. No gateway routing here.

use gproxy_protocol::{HttpBody, WireRequest, WireResponse, capability::UpstreamConnection};
use http::Method;

use super::{ChannelError, CredentialContext, OperationFuture};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceTransport {
    Http,
    WebSocket,
}

#[derive(Debug, Clone)]
pub struct ServiceRoute {
    pub method: Method,
    /// Vendor path template, e.g. /api/oauth/files/{file_id}/content.
    /// The server interprets templates; the base does not match or whitelist them.
    pub path_template: &'static str,
    pub transport: ServiceTransport,
}

pub struct ServiceContext<'a, B = HttpBody> {
    pub account: CredentialContext<'a>,
    pub request: WireRequest<B>,
}

/// Route declarations are metadata for server integration, not an execution gate.
/// Implementations can forward, compose calls or produce local responses. Local
/// aggregation, gateway OAuth issuance and durable affinity need host services;
/// they are not implied by this upstream-service contract.
pub trait ChannelServices: Send + Sync {
    fn routes(&self) -> &[ServiceRoute] {
        &[]
    }

    fn call<'a>(
        &'a self,
        _context: ServiceContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async { Err(ChannelError::UnsupportedService) })
    }

    fn connect<'a>(
        &'a self,
        _context: ServiceContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        Box::pin(async { Err(ChannelError::UnsupportedService) })
    }
}
