//! Core-side implementations of protocol's host capabilities. Adaptation flows
//! that issue several physical calls receive these instead of a raw client, so
//! every exchange still passes through the attempt's credential binding, the
//! provider's operation URL, observation and the instance limits. Limits are
//! supplied explicitly by the execution path that constructs each instance;
//! there is no implicit unlimited configuration. Bodies return NotImplemented.

use crate::{AttemptContext, Core, ExecutionTarget};
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, CasResult, PublicationKind, PublicationStatus, PublishedResource,
        ResourceAccess, ResourceMetadata, ResourceRead, ResourceReference, StateEntry, StateStore,
        StateWrite, Upstream, UpstreamConnection, Version,
    },
};
use std::sync::Arc;

fn not_implemented<T>(name: &'static str) -> CapabilityFuture<'static, Result<T, CapabilityError>>
where
    T: 'static,
{
    Box::pin(async move {
        Err(CapabilityError::new(
            CapabilityErrorKind::Unsupported,
            CapabilityErrorStage::Start,
            format!("core capability `{name}` is not implemented"),
        ))
    })
}

/// Upstream calls bound to one attempt: its provider, pinned credential
/// version and assigned client. The target is the native operation to dispatch;
/// provider and credential are never chosen per call.
pub struct AttemptUpstream<'a, C> {
    core: &'a Core<C>,
    attempt: Arc<AttemptContext>,
    limits: CapabilityLimits,
}
impl<'a, C> AttemptUpstream<'a, C> {
    pub fn new(core: &'a Core<C>, attempt: Arc<AttemptContext>, limits: CapabilityLimits) -> Self {
        Self {
            core,
            attempt,
            limits,
        }
    }
    pub fn attempt(&self) -> &Arc<AttemptContext> {
        &self.attempt
    }
    pub fn core(&self) -> &'a Core<C> {
        self.core
    }
}
impl<C> Upstream for AttemptUpstream<'_, C> {
    type Target = OperationKey;

    /// Allocate an ExchangeContext, apply the provider's operation URL, reject
    /// absolute URLs and source authentication, dispatch through the channel
    /// with the pinned credential, and wrap the client for capture/usage.
    /// Non-2xx remains a response. No retry or credential change happens here.
    fn send<'a>(
        &'a self,
        target: &'a Self::Target,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        let _ = (target, request);
        not_implemented("AttemptUpstream::send")
    }

    /// Same binding rules over the channel's WS dispatch. A rejected handshake
    /// keeps its full HTTP response; a connected socket is observed per frame.
    fn connect<'a>(
        &'a self,
        target: &'a Self::Target,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        let _ = (target, request);
        not_implemented("AttemptUpstream::connect")
    }

    fn limits(&self) -> CapabilityLimits {
        self.limits
    }
}

/// Identity boundary for protocol continuation state (ID maps, signatures,
/// stream occupancy). Core serializes it into the persisted scope column;
/// adapters never compose that text. Credential attribution lives in the
/// payload record, so an agent session reassigned to another credential of the
/// same provider keeps its state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StateScope {
    /// Opaque caller isolation scope from RequestContext.
    pub scope: String,
    pub provider_id: String,
    /// Adapter-supplied conversation key, e.g. a Responses chain or WS lane.
    pub conversation: Option<String>,
}

/// StateStore over Store's ProtocolState rows. Versions are host-generated and
/// never reused across delete/recreate; expired rows read as absent.
pub struct ProtocolState<'a, C> {
    core: &'a Core<C>,
    limits: CapabilityLimits,
}
impl<'a, C> ProtocolState<'a, C> {
    pub fn new(core: &'a Core<C>, limits: CapabilityLimits) -> Self {
        Self { core, limits }
    }
    pub fn core(&self) -> &'a Core<C> {
        self.core
    }
}
impl<C> StateStore for ProtocolState<'_, C> {
    type Scope = StateScope;

    fn get<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        let _ = (scope, key);
        not_implemented("ProtocolState::get")
    }

    fn compare_exchange<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        let _ = (scope, key, expected, replacement);
        not_implemented("ProtocolState::compare_exchange")
    }

    fn limits(&self) -> CapabilityLimits {
        self.limits
    }
}

/// Which upstream a resource lives at and which credentials may touch it.
/// The upper layer supplies the target; core never widens it to the provider
/// pool. A source scope may name a different provider than the request target.
pub struct ResourceScope {
    /// Opaque caller isolation scope from RequestContext.
    pub scope: String,
    pub target: ExecutionTarget,
}

/// Release handle referencing the ResourceBinding row. Release re-verifies
/// scope ownership from the row; the handle carries no bearer secret.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PublishedHandle {
    pub binding_id: String,
}

/// ResourceAccess over Store's ResourceBinding/FileObject rows and the file
/// backend. Upstream reads go through the scope's provider and credentials.
pub struct Resources<'a, C> {
    core: &'a Core<C>,
    limits: CapabilityLimits,
}
impl<'a, C> Resources<'a, C> {
    pub fn new(core: &'a Core<C>, limits: CapabilityLimits) -> Self {
        Self { core, limits }
    }
    pub fn core(&self) -> &'a Core<C> {
        self.core
    }
}
impl<C> ResourceAccess for Resources<'_, C> {
    type Scope = ResourceScope;
    type PublishedHandle = PublishedHandle;

    fn resolve<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        let _ = (scope, reference);
        not_implemented("Resources::resolve")
    }

    fn read<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        let _ = (scope, reference);
        not_implemented("Resources::read")
    }

    /// Idempotent per (scope, operation_id) through a ResourceBinding row with
    /// its expiry; rejection and duplicate paths do not consume the body.
    fn publish<'a>(
        &'a self,
        scope: &'a Self::Scope,
        operation_id: &'a str,
        kind: PublicationKind,
        metadata: ResourceMetadata,
        body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<Self::PublishedHandle>, CapabilityError>>
    {
        let _ = (scope, operation_id, kind, metadata, body);
        not_implemented("Resources::publish")
    }

    fn publication_status<'a>(
        &'a self,
        scope: &'a Self::Scope,
        operation_id: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<Self::PublishedHandle>, CapabilityError>>
    {
        let _ = (scope, operation_id);
        not_implemented("Resources::publication_status")
    }

    fn release<'a>(
        &'a self,
        scope: &'a Self::Scope,
        handle: &'a Self::PublishedHandle,
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        let _ = (scope, handle);
        not_implemented("Resources::release")
    }

    fn limits(&self) -> CapabilityLimits {
        self.limits
    }
}
