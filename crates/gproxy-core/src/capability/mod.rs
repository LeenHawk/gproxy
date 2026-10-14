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
use gproxy_channel::{ChannelBinding, ChannelError, channel::ChannelState};
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
#[derive(Clone)]
pub struct AttemptUpstream {
    funnel: Arc<Funnel>,
    attempt: Arc<AttemptContext>,
    inbound_headers: HeaderMap,
    limits: CapabilityLimits,
    channel_state: Arc<dyn ChannelState>,
    instance_id: Arc<str>,
}
impl AttemptUpstream {
    pub(crate) fn new(
        funnel: Arc<Funnel>,
        attempt: Arc<AttemptContext>,
        inbound_headers: HeaderMap,
        limits: CapabilityLimits,
        channel_state: Arc<dyn ChannelState>,
        instance_id: Arc<str>,
    ) -> Self {
        Self {
            funnel,
            attempt,
            inbound_headers,
            limits,
            channel_state,
            instance_id,
        }
    }
    pub fn attempt(&self) -> &Arc<AttemptContext> {
        &self.attempt
    }
}
impl Upstream for AttemptUpstream {
    type Target = OperationKey;

    fn send<'a>(
        &'a self,
        target: &'a Self::Target,
        mut request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let request_context = &self.attempt.request;
            let provider = &request_context.target.provider;
            let context = RewriteContext {
                operation: *target,
                upstream_model: request_context.target.upstream_model.as_deref(),
                requested_model: request_context.target.requested_model.as_deref(),
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
            let mut response = crate::execute::NativeCall {
                funnel: self.funnel.clone(),
                attempt: self.attempt.clone(),
                operation: *target,
                response_rules: response_rules.body.clone(),
                limits: self.limits,
                state: self.channel_state.clone(),
                instance_id: self.instance_id.clone(),
            }
            .send(request)
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
                requested_model: request_context.target.requested_model.as_deref(),
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
            let observed =
                ObservedClient::new(credential.websocket_client.clone(), exchange.clone());
            let endpoint = provider.operation_url_for(
                *target,
                EndpointTransport::WebSocket,
                request_context.target.upstream_model.as_deref(),
            );
            let binding = ChannelBinding::new(
                provider.channel.as_ref(),
                prepare::provider_view(provider),
                prepare::credential_view(credential, &self.attempt.credential_version),
                Arc::new(observed),
            )
            .state(self.channel_state.clone())
            .instance(self.instance_id.clone())
            .endpoint(endpoint.as_deref());
            let connection = binding
                .connect(*target, request)
                .await
                .map_err(channel_error)?;
            Ok(match connection {
                UpstreamConnection::Connected { handshake, socket } => {
                    UpstreamConnection::Connected {
                        handshake,
                        socket: crate::execute::observe_generation_socket(
                            exchange,
                            socket,
                            &request_rules,
                            self.limits.ws_frame_bytes,
                        ),
                    }
                }
                rejected => rejected,
            })
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

/// A channel's cross-request memory: protocol state rows under a scope core
/// fixes to one provider and one credential, so a channel can only ever read
/// and write its own account's keys. Built per binding by the execution path.
pub struct ChannelStateStore<C> {
    state: ProtocolState<C>,
    scope: StateScope,
}
impl<C> ChannelStateStore<C> {
    pub(crate) fn new(state: ProtocolState<C>, provider_id: &str, credential_id: &str) -> Self {
        Self {
            state,
            scope: StateScope {
                scope: "channel".to_owned(),
                provider_id: provider_id.to_owned(),
                conversation: Some(credential_id.to_owned()),
            },
        }
    }
}
impl<C: BatchConnectionTrait + Send + Sync> ChannelState for ChannelStateStore<C> {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        self.state.get(&self.scope, key)
    }
    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        self.state
            .compare_exchange(&self.scope, key, expected, replacement)
    }
    fn limits(&self) -> CapabilityLimits {
        self.state.limits()
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

impl ResourceScope {
    /// The persisted scope column: caller scope plus the provider the resource
    /// belongs to, so one caller's publications never alias across providers.
    pub fn column(&self) -> String {
        format!("{}\u{1f}{}", self.scope, self.target.provider.entity.id)
    }
}

/// Release handle referencing the ResourceBinding row. Release re-verifies
/// scope ownership from the row; the handle carries no bearer secret.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PublishedHandle {
    pub binding_id: String,
}

mod resources;
pub(crate) use resources::PUBLICATION_KIND;
pub use resources::Resources;
