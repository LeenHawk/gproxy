//! Default execution shared by the explicitly named operation methods.

use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityFuture, UpstreamConnection},
};

use super::{BaseChannel, ChannelError, CredentialView, PrepareContext, ProviderView};
use crate::client::OutboundClient;

pub type OperationFuture<'a, T> = CapabilityFuture<'a, Result<T, ChannelError>>;

/// The exact provider, credential and client chosen by the caller. The named
/// method determines the operation; dialect selects its wire format. Overrides
/// may implement local responses or multiple calls through the assigned client.
/// They must not secretly create clients, select credentials or retry side effects.
pub struct OperationContext<'a, B = HttpBody> {
    pub provider: ProviderView<'a>,
    pub credential: CredentialView<'a>,
    pub dialect: Dialect,
    pub request: WireRequest<B>,
    pub client: &'a dyn OutboundClient,
}

pub(super) fn http<'a>(
    channel: &'a (impl BaseChannel + ?Sized),
    operation: Operation,
    context: OperationContext<'a>,
) -> OperationFuture<'a, WireResponse<HttpBody>> {
    Box::pin(async move {
        let key = OperationKey {
            operation,
            dialect: context.dialect,
        };
        let request = channel.prepare(PrepareContext {
            provider: context.provider,
            credential: context.credential,
            operation: key,
            request: context.request,
        })?;
        context.client.send(request).await
    })
}

pub(super) fn websocket<'a>(
    channel: &'a (impl BaseChannel + ?Sized),
    operation: Operation,
    context: OperationContext<'a, ()>,
) -> OperationFuture<'a, UpstreamConnection> {
    Box::pin(async move {
        let key = OperationKey {
            operation,
            dialect: context.dialect,
        };
        let request = channel.prepare_connect(PrepareContext {
            provider: context.provider,
            credential: context.credential,
            operation: key,
            request: context.request,
        })?;
        context.client.connect(request).await
    })
}
