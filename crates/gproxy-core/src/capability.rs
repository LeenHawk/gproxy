//! Core-side implementations of protocol's host capabilities. Adaptation flows
//! that issue several physical calls receive these instead of a raw client, so
//! every exchange still passes through the attempt's credential binding, the
//! provider's operation URL, rewrite rules, observation and the instance
//! limits. Limits are supplied explicitly by the execution path that constructs
//! each instance; there is no implicit unlimited configuration.

use crate::{
    AttemptContext, Core, ExecutionTarget,
    api::lifecycle::now_ms,
    execute::{Exchange, Funnel, ObservedClient, prepare},
    rewrite::{Phase, RewriteContext, apply_body, apply_headers, apply_query, select_rules},
};
use gproxy_channel::{ChannelBinding, ChannelError};
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, CasResult, PublicationKind, PublicationStatus, PublishedResource,
        ResourceAccess, ResourceMetadata, ResourceRead, ResourceReference, StateEntry, StateStore,
        StateWrite, Upstream, UpstreamConnection, Version,
    },
    connection::Bytes,
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::upstream::operation_endpoint::EndpointTransport;
use gproxy_store::operations::state::{StateChange, StateOutcome, StateValue};
use http::HeaderMap;
use std::{sync::Arc, time::SystemTime};

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

fn channel_error(error: ChannelError) -> CapabilityError {
    match error {
        ChannelError::Transport(error) => error,
        ChannelError::UnsupportedOperation(_) | ChannelError::WrongTransport(_) => {
            CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                error.to_string(),
            )
        }
        ChannelError::InvalidConfig(_) | ChannelError::InvalidCredential => CapabilityError::new(
            CapabilityErrorKind::Invalid,
            CapabilityErrorStage::Start,
            error.to_string(),
        ),
        other => CapabilityError::new(
            CapabilityErrorKind::Transport,
            CapabilityErrorStage::Start,
            other.to_string(),
        ),
    }
}

fn invalid(message: String) -> CapabilityError {
    CapabilityError::new(
        CapabilityErrorKind::Invalid,
        CapabilityErrorStage::Start,
        message,
    )
}

/// Upstream calls bound to one attempt: its provider, pinned credential
/// version and assigned client. The target is the native operation to
/// dispatch; provider and credential are never chosen per call. Every call
/// allocates one observed exchange, applies the request rules selected for
/// that native operation, and routes through the channel binding.
pub struct AttemptUpstream<'a, C> {
    core: &'a Core<C>,
    funnel: Arc<Funnel>,
    attempt: Arc<AttemptContext>,
    inbound_headers: HeaderMap,
    limits: CapabilityLimits,
}
impl<'a, C> AttemptUpstream<'a, C> {
    pub(crate) fn new(
        core: &'a Core<C>,
        funnel: Arc<Funnel>,
        attempt: Arc<AttemptContext>,
        inbound_headers: HeaderMap,
        limits: CapabilityLimits,
    ) -> Self {
        Self {
            core,
            funnel,
            attempt,
            inbound_headers,
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
impl<C: Send + Sync> Upstream for AttemptUpstream<'_, C> {
    type Target = OperationKey;

    fn send<'a>(
        &'a self,
        target: &'a Self::Target,
        mut request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let request_context = &self.attempt.request;
            let provider = &request_context.target.provider;
            let credential = &self.attempt.credential;
            let context = RewriteContext {
                operation: *target,
                upstream_model: request_context.target.upstream_model.as_deref(),
                requested_model: None,
                request_headers: &self.inbound_headers,
            };
            let request_rules = select_rules(
                &request_context.snapshot,
                provider,
                Phase::Request,
                &context,
            );
            let response_rules = select_rules(
                &request_context.snapshot,
                provider,
                Phase::Response,
                &context,
            );
            if !request_rules.headers.is_empty() {
                apply_headers(&request_rules.headers, &mut request.headers)
                    .map_err(|e| invalid(e.to_string()))?;
            }
            if !request_rules.query.is_empty()
                && let Some(query) = apply_query(&request_rules.query, request.query.as_deref())
                    .map_err(|e| invalid(e.to_string()))?
            {
                request.query = Some(query);
            }
            if !request_rules.body.is_empty()
                && let HttpBody::Bytes(bytes) = &request.body
                && let Some(rewritten) =
                    apply_body(&request_rules.body, bytes).map_err(|e| invalid(e.to_string()))?
            {
                request.body = HttpBody::Bytes(Bytes::from(rewritten));
            }
            let exchange = Exchange::new(
                self.funnel.clone(),
                self.attempt.clone(),
                *target,
                provider.channel.clone(),
                response_rules.body.clone(),
                self.limits,
                now_ms(),
            );
            let observed = ObservedClient::new(credential.client.as_ref(), exchange);
            let binding = ChannelBinding::new(
                provider.channel.as_ref(),
                prepare::provider_view(provider),
                prepare::credential_view(credential, &self.attempt.credential_version),
                &observed,
            )
            .endpoint(provider.operation_url(*target, EndpointTransport::Http));
            let mut response = binding
                .send(*target, request)
                .await
                .map_err(channel_error)?;
            if !response_rules.headers.is_empty() {
                apply_headers(&response_rules.headers, &mut response.headers)
                    .map_err(|e| invalid(e.to_string()))?;
            }
            Ok(response)
        })
    }

    fn connect<'a>(
        &'a self,
        target: &'a Self::Target,
        mut request: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            let request_context = &self.attempt.request;
            let provider = &request_context.target.provider;
            let credential = &self.attempt.credential;
            let context = RewriteContext {
                operation: *target,
                upstream_model: request_context.target.upstream_model.as_deref(),
                requested_model: None,
                request_headers: &self.inbound_headers,
            };
            let request_rules = select_rules(
                &request_context.snapshot,
                provider,
                Phase::Request,
                &context,
            );
            let response_rules = select_rules(
                &request_context.snapshot,
                provider,
                Phase::Response,
                &context,
            );
            if !request_rules.headers.is_empty() {
                apply_headers(&request_rules.headers, &mut request.headers)
                    .map_err(|e| invalid(e.to_string()))?;
            }
            if !request_rules.query.is_empty()
                && let Some(query) = apply_query(&request_rules.query, request.query.as_deref())
                    .map_err(|e| invalid(e.to_string()))?
            {
                request.query = Some(query);
            }
            let exchange = Exchange::new(
                self.funnel.clone(),
                self.attempt.clone(),
                *target,
                provider.channel.clone(),
                response_rules.body.clone(),
                self.limits,
                now_ms(),
            );
            let observed = ObservedClient::new(credential.websocket_client.as_ref(), exchange);
            let binding = ChannelBinding::new(
                provider.channel.as_ref(),
                prepare::provider_view(provider),
                prepare::credential_view(credential, &self.attempt.credential_version),
                &observed,
            )
            .endpoint(provider.operation_url(*target, EndpointTransport::WebSocket));
            binding
                .connect(*target, request)
                .await
                .map_err(channel_error)
        })
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

impl StateScope {
    /// The persisted scope column. Unit separators keep the three parts
    /// unambiguous without escaping.
    pub fn column(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            self.scope,
            self.provider_id,
            self.conversation.as_deref().unwrap_or("")
        )
    }
}

fn storage(error: gproxy_store::StoreError) -> CapabilityError {
    CapabilityError::with_source(
        CapabilityErrorKind::Storage,
        CapabilityErrorStage::Start,
        "protocol state storage failed",
        error,
    )
}

fn ms_to_time(ms: i64) -> SystemTime {
    SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

fn time_to_ms(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// StateStore over Store's ProtocolState rows. Versions are host-generated
/// receipts and never reused across delete/recreate; expired rows read as
/// absent in both reads and CAS. Owns its Store handle so a driven client
/// stream can keep writing continuation state after the attempt returned.
pub struct ProtocolState<C> {
    store: Arc<gproxy_store::Store<C>>,
    limits: CapabilityLimits,
}
impl<C> Clone for ProtocolState<C> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            limits: self.limits,
        }
    }
}
impl<C> ProtocolState<C> {
    pub fn new(core: &Core<C>, limits: CapabilityLimits) -> Self {
        Self {
            store: core.store().clone(),
            limits,
        }
    }
}
impl<C: BatchConnectionTrait + Send + Sync> StateStore for ProtocolState<C> {
    type Scope = StateScope;

    fn get<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move {
            let rows = self
                .store
                .protocol_states()
                .get_live_many(&[(scope.column(), key.to_owned())], now_ms())
                .await
                .map_err(storage)?;
            Ok(rows.into_iter().next().flatten().map(|row| StateEntry {
                payload: Bytes::from(row.payload),
                version: Version::from_bytes(row.version),
                expires_at: row.expires_at_ms.map(ms_to_time),
            }))
        })
    }

    fn compare_exchange<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let outcomes = self
                .store
                .protocol_states()
                .compare_exchange_many(
                    vec![StateChange {
                        scope: scope.column(),
                        key: key.to_owned(),
                        expected: expected.map(|v| v.as_bytes().to_vec()),
                        replacement: replacement.map(|write| StateValue {
                            payload: write.payload.to_vec(),
                            expires_at_ms: write.expires_at.map(time_to_ms),
                        }),
                    }],
                    now_ms(),
                )
                .await
                .map_err(storage)?;
            Ok(match outcomes.into_iter().next() {
                Some(StateOutcome::Applied(version)) => {
                    CasResult::Applied(version.map(Version::from_bytes))
                }
                Some(StateOutcome::Conflict) | None => CasResult::Conflict,
            })
        })
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
/// Bodies return NotImplemented until the files/images families land.
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
