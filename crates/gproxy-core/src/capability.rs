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
use gproxy_store::entity::resource::{file_object, resource_binding};
use gproxy_store::entity::upstream::operation_endpoint::EndpointTransport;
use gproxy_store::operations::state::{StateChange, StateOutcome, StateValue};
use http::HeaderMap;
use sea_orm::ActiveValue::Set;
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
}
impl AttemptUpstream {
    pub(crate) fn new(
        funnel: Arc<Funnel>,
        attempt: Arc<AttemptContext>,
        inbound_headers: HeaderMap,
        limits: CapabilityLimits,
    ) -> Self {
        Self {
            funnel,
            attempt,
            inbound_headers,
            limits,
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

/// `resource_bindings.kind` for bodies core published itself.
const PUBLICATION_KIND: &str = "publication";
/// `file_objects.storage_name` for the configured file backend.
const STORAGE_NAME: &str = "core";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum PublicationState {
    /// Row inserted, body not yet stored. A cancelled or failed publish leaves
    /// this until its expiry passes.
    Pending,
    Published,
    /// Tombstone: released, failed, or noticed past expiry. Never reused.
    Expired,
}

/// `resource_bindings.summary` for publication rows.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct PublicationSummary {
    state: PublicationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    length: Option<u64>,
}

type BindingRow = resource_binding::Model;

fn resource_error(kind: CapabilityErrorKind, message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(kind, CapabilityErrorStage::Start, message)
}

fn resource_storage(error: gproxy_store::StoreError) -> CapabilityError {
    CapabilityError::with_source(
        CapabilityErrorKind::Storage,
        CapabilityErrorStage::Start,
        "resource binding storage failed",
        error,
    )
}

fn file_backend(error: gproxy_file::Error) -> CapabilityError {
    CapabilityError::with_source(
        CapabilityErrorKind::Storage,
        CapabilityErrorStage::BodyTransfer,
        "file backend failed",
        error,
    )
}

fn transform_error(error: gproxy_protocol::transform::TransformError) -> CapabilityError {
    use gproxy_protocol::transform::TransformErrorKind as K;
    let kind = match error.kind() {
        K::Limit => CapabilityErrorKind::Limit,
        K::Host => CapabilityErrorKind::Transport,
        K::Unsupported => CapabilityErrorKind::Unsupported,
        K::Conflict => CapabilityErrorKind::Conflict,
        K::InvalidInput | K::InvalidResult | K::MissingMetadata | K::MissingState => {
            CapabilityErrorKind::Invalid
        }
    };
    CapabilityError::with_source(kind, CapabilityErrorStage::Start, error.to_string(), error)
}

/// An upstream non-2xx answer to a resource read. The body is dropped: a
/// resource read has no caller to hand the provider's error document to.
fn rejected(status: http::StatusCode) -> CapabilityError {
    let kind = match status {
        http::StatusCode::NOT_FOUND | http::StatusCode::GONE => CapabilityErrorKind::NotFound,
        s if s.is_client_error() => CapabilityErrorKind::Invalid,
        _ => CapabilityErrorKind::Transport,
    };
    resource_error(
        kind,
        format!("upstream answered {status} to a resource read"),
    )
}

fn summary_of(row: &BindingRow) -> Result<PublicationSummary, CapabilityError> {
    serde_json::from_value(row.summary.clone()).map_err(|e| {
        resource_error(
            CapabilityErrorKind::Storage,
            format!("publication row {} has an unreadable summary: {e}", row.id),
        )
    })
}

fn row_expired(row: &BindingRow, now: i64) -> bool {
    row.expires_at_ms.is_some_and(|at| at <= now)
}

fn published_of(
    row: &BindingRow,
    summary: &PublicationSummary,
) -> PublishedResource<PublishedHandle> {
    PublishedResource {
        handle: PublishedHandle {
            binding_id: row.id.clone(),
        },
        reference: ResourceReference::Id(row.id.clone()),
        metadata: ResourceMetadata {
            mime: summary.mime.clone(),
            length: summary.length,
            filename: summary.filename.clone(),
            expires_at: row.expires_at_ms.map(ms_to_time),
        },
    }
}

fn object_key(binding_id: &str) -> String {
    format!("publications/{binding_id}")
}

/// One `Upstream` bound to a resource scope's provider and its first usable
/// credential. Resource reads are not part of any attempt: they apply the
/// provider's request rewrite rules and channel binding but are not observed
/// and never count toward the request's availability bookkeeping.
struct ScopeUpstream<'a, C> {
    core: &'a Core<C>,
    scope: &'a ResourceScope,
    limits: CapabilityLimits,
}

impl<C> ScopeUpstream<'_, C> {
    fn credential(&self) -> Result<&Arc<crate::CredentialData>, CapabilityError> {
        self.scope
            .target
            .credentials
            .iter()
            .find(|c| c.enabled && !c.state.is_retired())
            .ok_or_else(|| {
                resource_error(
                    CapabilityErrorKind::Invalid,
                    "resource scope has no usable credential",
                )
            })
    }
}

impl<C: Send + Sync> Upstream for ScopeUpstream<'_, C> {
    type Target = OperationKey;

    fn send<'a>(
        &'a self,
        target: &'a Self::Target,
        mut request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let provider = &self.scope.target.provider;
            let credential = self.credential()?;
            let version = credential.state.load();
            let snapshot = self.core.snapshot();
            let empty = HeaderMap::new();
            let context = RewriteContext {
                operation: *target,
                upstream_model: self.scope.target.upstream_model.as_deref(),
                requested_model: None,
                request_headers: &empty,
            };
            let request_rules = select_rules(&snapshot, provider, Phase::Request, &context);
            let response_rules = select_rules(&snapshot, provider, Phase::Response, &context);
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
            let binding = ChannelBinding::new(
                provider.channel.as_ref(),
                prepare::provider_view(provider),
                prepare::credential_view(credential, &version),
                credential.client.as_ref(),
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
        _: &'a Self::Target,
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async {
            Err(resource_error(
                CapabilityErrorKind::Unsupported,
                "resource access has no WebSocket operations",
            ))
        })
    }

    fn limits(&self) -> CapabilityLimits {
        self.limits
    }
}

/// GET template for `path`. Mirrors `convert::endpoints`, which is private to
/// the conversion families; resource reads share the same native paths.
fn template(path: String) -> WireRequest<()> {
    WireRequest {
        method: http::Method::GET,
        path,
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}

/// The files collection of `dialect` (see `convert::endpoints::files_endpoint`).
fn files_path(dialect: gproxy_protocol::Dialect) -> Result<String, CapabilityError> {
    use gproxy_protocol::Dialect;
    match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::Claude => Ok("/v1/files".into()),
        Dialect::Gemini => Ok("/v1beta/files".into()),
        Dialect::OpenAiResponsesWebSocket => Err(resource_error(
            CapabilityErrorKind::Unsupported,
            "the Responses WebSocket dialect has no files API",
        )),
    }
}

/// One file's content download; Gemini files are input-only upstream.
fn file_content_path(
    dialect: gproxy_protocol::Dialect,
    id: &str,
) -> Result<String, CapabilityError> {
    use gproxy_protocol::Dialect;
    if id.is_empty() || id.contains(['/', '?', '#', '\\', '\r', '\n']) {
        return Err(resource_error(
            CapabilityErrorKind::Invalid,
            "file id must be one path segment",
        ));
    }
    match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::Claude => {
            Ok(format!("/v1/files/{id}/content"))
        }
        Dialect::Gemini => Err(resource_error(
            CapabilityErrorKind::Unsupported,
            "Gemini files cannot be downloaded",
        )),
        Dialect::OpenAiResponsesWebSocket => Err(resource_error(
            CapabilityErrorKind::Unsupported,
            "the Responses WebSocket dialect has no files API",
        )),
    }
}

/// ResourceAccess over Store's `resource_bindings`/`file_objects` rows and the
/// configured file backend.
///
/// * `publish` stores the body through `Core::file_storage` and records one
///   `publication` binding per `(scope, operation_id)`. Only
///   `PublicationKind::Id` is supported: core has no public URL surface, so a
///   `Url` publication is rejected before the body is touched. Without a file
///   backend every publish is `Unsupported` before side effects.
/// * `ResourceReference::Id` first resolves to a publication in the same scope;
///   any other id is read from the scope's provider through its channel with
///   the first usable credential of the scope target. `Url` references are
///   rejected as `Unsupported`: there is no host allow-list to authorise them.
pub struct Resources<'a, C> {
    core: &'a Core<C>,
    limits: CapabilityLimits,
    codec: gproxy_protocol::codec::CodecLimits,
}
impl<'a, C> Resources<'a, C> {
    pub fn new(
        core: &'a Core<C>,
        limits: CapabilityLimits,
        codec: gproxy_protocol::codec::CodecLimits,
    ) -> Self {
        Self {
            core,
            limits,
            codec,
        }
    }
    pub fn core(&self) -> &'a Core<C> {
        self.core
    }
}

impl<C: BatchConnectionTrait + Send + Sync> Resources<'_, C> {
    fn bindings(&self) -> gproxy_store::Repository<'_, C, resource_binding::Entity> {
        self.core.store().resource_bindings()
    }

    fn files(&self) -> gproxy_store::Repository<'_, C, file_object::Entity> {
        self.core.store().file_objects()
    }

    fn backend(&self) -> Result<&gproxy_file::Operator, CapabilityError> {
        self.core.file_storage().ok_or_else(|| {
            resource_error(
                CapabilityErrorKind::Unsupported,
                "no file storage configured for locally published resources",
            )
        })
    }

    async fn publication(
        &self,
        column: &str,
        operation_id: &str,
    ) -> Result<Option<BindingRow>, CapabilityError> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        let rows = self
            .bindings()
            .query(
                resource_binding::Entity::find()
                    .filter(resource_binding::Column::Scope.eq(column))
                    .filter(resource_binding::Column::Kind.eq(PUBLICATION_KIND))
                    .filter(resource_binding::Column::PublicId.eq(operation_id))
                    .filter(resource_binding::Column::Generation.eq(0i64)),
            )
            .await
            .map_err(resource_storage)?;
        Ok(rows.into_iter().next())
    }

    /// The publication row behind a handle or an `Id` reference, only when it
    /// belongs to `scope`. A row owned by another scope reads as absent.
    async fn owned(
        &self,
        scope: &ResourceScope,
        binding_id: &str,
    ) -> Result<Option<BindingRow>, CapabilityError> {
        let row = self
            .bindings()
            .get_many(&[binding_id.to_owned()])
            .await
            .map_err(resource_storage)?
            .into_iter()
            .next()
            .flatten();
        Ok(row.filter(|row| row.kind == PUBLICATION_KIND && row.scope == scope.column()))
    }

    async fn set_state(
        &self,
        row: &BindingRow,
        summary: &mut PublicationSummary,
        state: PublicationState,
        now: i64,
    ) -> Result<(), CapabilityError> {
        summary.state = state;
        let value = serde_json::to_value(&*summary).map_err(|e| {
            resource_error(CapabilityErrorKind::Storage, format!("summary encode: {e}"))
        })?;
        self.bindings()
            .update_many(vec![resource_binding::ActiveModel {
                id: Set(row.id.clone()),
                summary: Set(value),
                updated_at_ms: Set(now),
                ..Default::default()
            }])
            .await
            .map_err(resource_storage)?;
        Ok(())
    }

    /// Store-side view of one publication row, tombstoning it when its expiry
    /// has passed so later reads never see a stale `Published`.
    async fn status_of(
        &self,
        row: &BindingRow,
        now: i64,
    ) -> Result<PublicationStatus<PublishedHandle>, CapabilityError> {
        let mut summary = summary_of(row)?;
        if summary.state != PublicationState::Expired && row_expired(row, now) {
            self.set_state(row, &mut summary, PublicationState::Expired, now)
                .await?;
        }
        Ok(match summary.state {
            PublicationState::Pending => PublicationStatus::Pending,
            PublicationState::Expired => PublicationStatus::Expired,
            PublicationState::Published => {
                PublicationStatus::Published(published_of(row, &summary))
            }
        })
    }

    async fn local_metadata(
        &self,
        scope: &ResourceScope,
        binding_id: &str,
    ) -> Result<Option<(BindingRow, ResourceMetadata)>, CapabilityError> {
        let Some(row) = self.owned(scope, binding_id).await? else {
            return Ok(None);
        };
        match self.status_of(&row, now_ms()).await? {
            PublicationStatus::Published(published) => Ok(Some((row, published.metadata))),
            PublicationStatus::Expired => Err(resource_error(
                CapabilityErrorKind::Expired,
                "publication expired",
            )),
            PublicationStatus::Pending | PublicationStatus::Missing => Err(resource_error(
                CapabilityErrorKind::NotFound,
                "publication has no stored body",
            )),
        }
    }

    fn upstream<'s>(&'s self, scope: &'s ResourceScope) -> ScopeUpstream<'s, C> {
        ScopeUpstream {
            core: self.core,
            scope,
            limits: self.limits,
        }
    }

    /// The dialect the scope's provider speaks for `operation`, from the
    /// channel's declaration. Operation rules cannot override it here: a
    /// resource read has no request context of its own.
    fn native_dialect(
        &self,
        scope: &ResourceScope,
        operation: gproxy_protocol::Operation,
    ) -> Result<gproxy_protocol::Dialect, CapabilityError> {
        let provider = &scope.target.provider;
        provider
            .channel
            .native_dialects(prepare::provider_view(provider), operation)
            .into_iter()
            .next()
            .ok_or_else(|| {
                resource_error(
                    CapabilityErrorKind::Unsupported,
                    format!(
                        "provider {} declares no native dialect for {operation:?}",
                        provider.entity.id
                    ),
                )
            })
    }

    async fn upstream_metadata(
        &self,
        scope: &ResourceScope,
        id: &str,
    ) -> Result<ResourceMetadata, CapabilityError> {
        use gproxy_protocol::{Dialect, Operation, adapt::files};
        let dialect = self.native_dialect(scope, Operation::RetrieveFile)?;
        let key = OperationKey {
            operation: Operation::RetrieveFile,
            dialect,
        };
        let template = template(files_path(dialect)?);
        let upstream = self.upstream(scope);
        let metadata = match dialect {
            Dialect::OpenAi | Dialect::OpenAiChat => {
                match files::openai_get(&upstream, &key, template, id, self.codec)
                    .await
                    .map_err(transform_error)?
                {
                    gproxy_protocol::adapt::JsonInvocation::Rejected(r) => {
                        return Err(rejected(r.status));
                    }
                    gproxy_protocol::adapt::JsonInvocation::Success(r) => ResourceMetadata {
                        mime: None,
                        length: u64::try_from(r.body.bytes).ok(),
                        filename: Some(r.body.filename),
                        expires_at: r
                            .body
                            .expires_at
                            .flatten()
                            .map(|s| ms_to_time(s.saturating_mul(1000))),
                    },
                }
            }
            Dialect::Claude => {
                match files::claude_get(&upstream, &key, template, id, self.codec)
                    .await
                    .map_err(transform_error)?
                {
                    gproxy_protocol::adapt::JsonInvocation::Rejected(r) => {
                        return Err(rejected(r.status));
                    }
                    gproxy_protocol::adapt::JsonInvocation::Success(r) => ResourceMetadata {
                        mime: Some(r.body.mime_type),
                        length: u64::try_from(r.body.size_bytes).ok(),
                        filename: Some(r.body.filename),
                        expires_at: None,
                    },
                }
            }
            Dialect::Gemini => {
                match files::gemini_get(&upstream, &key, template, id, self.codec)
                    .await
                    .map_err(transform_error)?
                {
                    gproxy_protocol::adapt::JsonInvocation::Rejected(r) => {
                        return Err(rejected(r.status));
                    }
                    gproxy_protocol::adapt::JsonInvocation::Success(r) => {
                        use gproxy_protocol::gemini::files::FileSize;
                        let file = r.body;
                        ResourceMetadata {
                            mime: file.mime_type.flatten(),
                            length: file.size_bytes.flatten().and_then(|s| match s {
                                FileSize::Integer(v) => u64::try_from(v).ok(),
                                FileSize::String(v) => v.parse().ok(),
                            }),
                            filename: file.display_name.flatten(),
                            expires_at: file
                                .expiration_time
                                .flatten()
                                .and_then(|t| httpdate_rfc3339(&t)),
                        }
                    }
                }
            }
            Dialect::OpenAiResponsesWebSocket => {
                return Err(resource_error(
                    CapabilityErrorKind::Unsupported,
                    "the Responses WebSocket dialect has no files API",
                ));
            }
        };
        Ok(metadata)
    }

    async fn upstream_read(
        &self,
        scope: &ResourceScope,
        id: &str,
    ) -> Result<ResourceRead, CapabilityError> {
        use gproxy_protocol::Operation;
        let dialect = self.native_dialect(scope, Operation::RetrieveFileContent)?;
        let path = file_content_path(dialect, id)?;
        let key = OperationKey {
            operation: Operation::RetrieveFileContent,
            dialect,
        };
        let metadata = self.upstream_metadata(scope, id).await?;
        let request = WireRequest {
            method: http::Method::GET,
            path,
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::new()),
        };
        let response = self.upstream(scope).send(&key, request).await?;
        if !response.status.is_success() {
            return Err(rejected(response.status));
        }
        Ok(ResourceRead {
            metadata,
            body: response.body,
        })
    }
}

/// RFC 3339 timestamps as Gemini emits them. Fractional seconds and offsets
/// beyond `Z` are not needed for an expiry check, so only the plain form parses.
fn httpdate_rfc3339(value: &str) -> Option<SystemTime> {
    let value = value.strip_suffix('Z')?;
    let (date, time) = value.split_once('T')?;
    let mut date = date.split('-');
    let (y, m, d) = (
        date.next()?.parse::<i64>().ok()?,
        date.next()?.parse::<i64>().ok()?,
        date.next()?.parse::<i64>().ok()?,
    );
    let time = time.split('.').next()?;
    let mut time = time.split(':');
    let (hh, mm, ss) = (
        time.next()?.parse::<i64>().ok()?,
        time.next()?.parse::<i64>().ok()?,
        time.next()?.parse::<i64>().ok()?,
    );
    // Days from civil, Howard Hinnant's algorithm.
    let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + hh * 3600 + mm * 60 + ss;
    u64::try_from(secs)
        .ok()
        .map(|s| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(s))
}

impl<C: BatchConnectionTrait + Send + Sync> ResourceAccess for Resources<'_, C> {
    type Scope = ResourceScope;
    type PublishedHandle = PublishedHandle;

    fn resolve<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        Box::pin(async move {
            let ResourceReference::Id(id) = reference else {
                return Err(resource_error(
                    CapabilityErrorKind::Unsupported,
                    "URL resources are not authorised: core has no host allow-list",
                ));
            };
            if let Some((_, metadata)) = self.local_metadata(scope, id).await? {
                return Ok(metadata);
            }
            self.upstream_metadata(scope, id).await
        })
    }

    fn read<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async move {
            let ResourceReference::Id(id) = reference else {
                return Err(resource_error(
                    CapabilityErrorKind::Unsupported,
                    "URL resources are not authorised: core has no host allow-list",
                ));
            };
            if let Some((row, metadata)) = self.local_metadata(scope, id).await? {
                let backend = self.backend()?;
                let bytes = backend
                    .read(&object_key(&row.id))
                    .await
                    .map_err(file_backend)?
                    .to_bytes();
                if bytes.len() as u64 > self.limits.read_bytes {
                    return Err(CapabilityError::new(
                        CapabilityErrorKind::Limit,
                        CapabilityErrorStage::BodyTransfer,
                        "stored body exceeds the read limit",
                    ));
                }
                return Ok(ResourceRead {
                    metadata,
                    body: HttpBody::Bytes(bytes),
                });
            }
            self.upstream_read(scope, id).await
        })
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
        Box::pin(async move {
            let column = scope.column();
            let now = now_ms();
            // Existing state wins over anything about the new request.
            if let Some(row) = self.publication(&column, operation_id).await? {
                return match self.status_of(&row, now).await? {
                    PublicationStatus::Published(published) => {
                        if matches!(
                            (&published.reference, kind),
                            (ResourceReference::Id(_), PublicationKind::Id)
                        ) {
                            Ok(published)
                        } else {
                            Err(resource_error(
                                CapabilityErrorKind::Conflict,
                                "operation id was published with another reference kind",
                            ))
                        }
                    }
                    PublicationStatus::Pending => Err(resource_error(
                        CapabilityErrorKind::Conflict,
                        "publication pending",
                    )),
                    PublicationStatus::Expired | PublicationStatus::Missing => Err(resource_error(
                        CapabilityErrorKind::Expired,
                        "publication expired",
                    )),
                };
            }
            match kind {
                PublicationKind::Id => {}
                PublicationKind::Url => {
                    return Err(resource_error(
                        CapabilityErrorKind::Unsupported,
                        "core publishes by id only: there is no public URL surface",
                    ));
                }
                #[allow(unreachable_patterns)]
                _ => {
                    return Err(resource_error(
                        CapabilityErrorKind::Unsupported,
                        "unknown publication kind",
                    ));
                }
            }
            let backend = self.backend()?;
            let Some(expires_at) = metadata.expires_at.filter(|at| time_to_ms(*at) > now) else {
                return Err(resource_error(
                    CapabilityErrorKind::Invalid,
                    "a publication needs an expiry in the future",
                ));
            };
            let expires_at_ms = time_to_ms(expires_at);
            let binding_id = crate::ids::random_id();
            let mut summary = PublicationSummary {
                state: PublicationState::Pending,
                mime: metadata.mime.clone(),
                filename: metadata.filename.clone(),
                length: metadata.length,
            };
            let credential_id = scope
                .target
                .credentials
                .first()
                .map(|c| c.id.clone())
                .unwrap_or_default();
            let inserted = self
                .bindings()
                .create_many(vec![resource_binding::ActiveModel {
                    id: Set(binding_id.clone()),
                    scope: Set(column.clone()),
                    kind: Set(PUBLICATION_KIND.into()),
                    public_id: Set(operation_id.to_owned()),
                    generation: Set(0),
                    assignment_id: Set(None),
                    provider_id: Set(scope.target.provider.entity.id.clone()),
                    upstream_id: Set(None),
                    user_id: Set(None),
                    credential_id: Set(credential_id),
                    parent_binding_id: Set(None),
                    secret: Set(None),
                    summary: Set(serde_json::to_value(&summary).unwrap_or_default()),
                    file_id: Set(None),
                    created_at_ms: Set(now),
                    updated_at_ms: Set(now),
                    expires_at_ms: Set(Some(expires_at_ms)),
                }])
                .await;
            let row = match inserted {
                Ok(rows) => rows.into_iter().next(),
                Err(error) => {
                    // The unique key lost a race: report the winner's state.
                    if self.publication(&column, operation_id).await?.is_some() {
                        return Err(resource_error(
                            CapabilityErrorKind::Conflict,
                            "publication pending",
                        ));
                    }
                    return Err(resource_storage(error));
                }
            };
            let Some(row) = row else {
                return Err(resource_error(
                    CapabilityErrorKind::Storage,
                    "publication row was not returned",
                ));
            };
            // From here a failure leaves a tombstone: the body may be partly
            // consumed and the operation id must never be replayed as fresh.
            let mut read_limits = self.codec;
            read_limits.max_body_bytes = read_limits.max_body_bytes.min(self.limits.write_bytes);
            let bytes = match gproxy_protocol::codec::read_http_body(body, read_limits).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.set_state(&row, &mut summary, PublicationState::Expired, now_ms())
                        .await?;
                    let kind = match error.kind() {
                        gproxy_protocol::codec::CodecErrorKind::Limit => CapabilityErrorKind::Limit,
                        _ => CapabilityErrorKind::Transport,
                    };
                    return Err(CapabilityError::with_source(
                        kind,
                        CapabilityErrorStage::BodyTransfer,
                        "publication body could not be read",
                        error,
                    ));
                }
            };
            if let Err(error) = backend.write(&object_key(&binding_id), bytes.clone()).await {
                self.set_state(&row, &mut summary, PublicationState::Expired, now_ms())
                    .await?;
                return Err(file_backend(error));
            }
            let length = bytes.len() as u64;
            let file = self
                .files()
                .create_many(vec![file_object::ActiveModel {
                    id: Set(crate::ids::random_id()),
                    storage_name: Set(STORAGE_NAME.into()),
                    object_key: Set(object_key(&binding_id)),
                    filename: Set(metadata.filename.clone()),
                    mime: Set(metadata.mime.clone()),
                    size_bytes: Set(i64::try_from(length).unwrap_or(i64::MAX)),
                    created_at_ms: Set(now),
                    expires_at_ms: Set(Some(expires_at_ms)),
                }])
                .await
                .map_err(resource_storage)?
                .into_iter()
                .next();
            summary.length = Some(length);
            summary.state = PublicationState::Published;
            let value = serde_json::to_value(&summary).unwrap_or_default();
            self.bindings()
                .update_many(vec![resource_binding::ActiveModel {
                    id: Set(binding_id.clone()),
                    file_id: Set(file.map(|f| f.id)),
                    summary: Set(value),
                    updated_at_ms: Set(now_ms()),
                    ..Default::default()
                }])
                .await
                .map_err(resource_storage)?;
            Ok(PublishedResource {
                handle: PublishedHandle {
                    binding_id: binding_id.clone(),
                },
                reference: ResourceReference::Id(binding_id),
                metadata: ResourceMetadata {
                    mime: metadata.mime,
                    length: Some(length),
                    filename: metadata.filename,
                    expires_at: Some(ms_to_time(expires_at_ms)),
                },
            })
        })
    }

    fn publication_status<'a>(
        &'a self,
        scope: &'a Self::Scope,
        operation_id: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<Self::PublishedHandle>, CapabilityError>>
    {
        Box::pin(async move {
            match self.publication(&scope.column(), operation_id).await? {
                None => Ok(PublicationStatus::Missing),
                Some(row) => self.status_of(&row, now_ms()).await,
            }
        })
    }

    fn release<'a>(
        &'a self,
        scope: &'a Self::Scope,
        handle: &'a Self::PublishedHandle,
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        Box::pin(async move {
            let Some(row) = self.owned(scope, &handle.binding_id).await? else {
                return Err(resource_error(
                    CapabilityErrorKind::Invalid,
                    "handle does not name a publication of this scope",
                ));
            };
            let now = now_ms();
            match self.status_of(&row, now).await? {
                PublicationStatus::Published(_) => {}
                PublicationStatus::Expired | PublicationStatus::Missing => {
                    return Err(resource_error(
                        CapabilityErrorKind::Expired,
                        "publication expired",
                    ));
                }
                PublicationStatus::Pending => {
                    return Err(resource_error(
                        CapabilityErrorKind::Invalid,
                        "publication is still pending",
                    ));
                }
            }
            // Tombstone first: a body that outlives its row is only garbage,
            // a row that outlives its body would serve a dangling reference.
            let mut summary = summary_of(&row)?;
            self.set_state(&row, &mut summary, PublicationState::Expired, now)
                .await?;
            if let Some(backend) = self.core.file_storage()
                && let Err(error) = backend.delete(&object_key(&row.id)).await
                && error.kind() != gproxy_file::ErrorKind::NotFound
            {
                return Err(file_backend(error));
            }
            Ok(())
        })
    }

    fn limits(&self) -> CapabilityLimits {
        self.limits
    }
}
