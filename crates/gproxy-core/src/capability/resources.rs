//! Publications and upstream file reads behind `ResourceAccess`, backed by
//! Store rows and the configured file storage (local filesystem natively,
//! S3-compatible storage such as R2 on every target).

use super::*;
use gproxy_store::entity::resource::{file_object, resource_binding};
use sea_orm::ActiveValue::Set;

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
    /// The host-built link of a `PublicationKind::Url` publication, so a
    /// replay by operation id returns the same reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
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

/// A file backend failure inside the `ResourceAccess` protocol trait, whose
/// error type is fixed. The message keeps the opendal kind so it survives
/// the `CapabilityError -> TransformError` flattening.
fn file_backend(error: gproxy_file::Error) -> CapabilityError {
    CapabilityError::with_source(
        CapabilityErrorKind::Storage,
        CapabilityErrorStage::BodyTransfer,
        format!("file backend failed ({})", error.kind()),
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

fn metadata_of(row: &BindingRow, summary: &PublicationSummary) -> ResourceMetadata {
    ResourceMetadata {
        mime: summary.mime.clone(),
        length: summary.length,
        filename: summary.filename.clone(),
        expires_at: row.expires_at_ms.map(ms_to_time),
    }
}

fn reference_of(row: &BindingRow, summary: &PublicationSummary) -> ResourceReference {
    match &summary.url {
        Some(url) => ResourceReference::Url(url.clone()),
        None => ResourceReference::Id(row.id.clone()),
    }
}

fn published_of(
    row: &BindingRow,
    summary: &PublicationSummary,
) -> PublishedResource<PublishedHandle> {
    PublishedResource {
        handle: PublishedHandle {
            binding_id: row.id.clone(),
        },
        reference: reference_of(row, summary),
        metadata: metadata_of(row, summary),
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
                requested_model: self.scope.target.requested_model.as_deref(),
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
                credential.client.clone(),
            )
            .instance(self.core.instance_id().clone())
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
///   `publication` binding per `(scope, operation_id)`. `PublicationKind::Id`
///   returns the binding id. `PublicationKind::Url` needs the host's
///   `PublicationUrl` (`CoreBuilder::publication_url`): the link is built for
///   the id core is about to record, before anything is written, so a host
///   refusal or a missing builder is `Unsupported` without side effects. The
///   host serves the bytes back through `Core::read_publication`. Without a
///   file backend every publish is `Unsupported` before side effects.
/// * `ResourceReference::Id` first resolves to a publication in the same scope;
///   any other id is read from the scope's provider through its channel with
///   the first usable credential of the scope target.
/// * `ResourceReference::Url` is fetched under the host's `FetchPolicy`
///   (`CoreBuilder::fetch_policy`, `DefaultFetchPolicy` unless set): `resolve`
///   is the policy check alone, `read` is the GET. The fetch goes through a
///   plain client of the core pool's default profile, never the scope's
///   credential client, so no provider auth leaks to an arbitrary origin;
///   redirects are followed up to `MAX_REDIRECT_HOPS` with the policy asked
///   again for every hop, and the body is bounded by the read limit.
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

/// Redirect hops followed for one URL read. The policy is asked again for
/// every hop, so a public link cannot bounce the fetch onto a private one.
const MAX_REDIRECT_HOPS: usize = 3;

fn unknown_reference() -> CapabilityError {
    resource_error(CapabilityErrorKind::Unsupported, "unknown reference kind")
}

fn parse_url(raw: &str) -> Result<url::Url, CapabilityError> {
    url::Url::parse(raw).map_err(|e| {
        resource_error(
            CapabilityErrorKind::Invalid,
            format!("URL resource is not a valid URL: {e}"),
        )
    })
}

fn transport(message: impl Into<String>) -> CapabilityError {
    resource_error(CapabilityErrorKind::Transport, message)
}

/// The addresses the runtime resolver returns for the URL's host name; empty
/// for a literal IP host (the policy reads that from the URL itself) and on
/// wasm32, which has no resolver. Natively a name that resolves to nothing
/// is a transport failure before any policy decision: there is nothing to
/// connect to.
#[cfg(not(target_arch = "wasm32"))]
async fn resolve_host(url: &url::Url) -> Result<Vec<std::net::IpAddr>, CapabilityError> {
    let Some(url::Host::Domain(name)) = url.host() else {
        return Ok(Vec::new());
    };
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<_> = tokio::net::lookup_host((name, port))
        .await
        .map_err(|e| transport(format!("URL host {name} did not resolve: {e}")))?
        .map(|addr| addr.ip())
        .collect();
    if addrs.is_empty() {
        return Err(transport(format!("URL host {name} resolved to no address")));
    }
    Ok(addrs)
}

#[cfg(target_arch = "wasm32")]
async fn resolve_host(_: &url::Url) -> Result<Vec<std::net::IpAddr>, CapabilityError> {
    Ok(Vec::new())
}

/// `Content-Type` without its parameters, the exact form adapters require.
fn mime_of(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(http::header::CONTENT_TYPE)?.to_str().ok()?;
    let mime = value.split(';').next()?.trim();
    (!mime.is_empty()).then(|| mime.to_owned())
}

/// The file name a `Content-Disposition` header carries: the RFC 5987
/// `filename*` form when present, else the plain `filename` parameter.
fn filename_of(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(http::header::CONTENT_DISPOSITION)?
        .to_str()
        .ok()?;
    let mut plain = None;
    for param in value.split(';').skip(1) {
        let Some((name, raw)) = param.split_once('=') else {
            continue;
        };
        let raw = raw.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "filename*" => {
                // charset'language'percent-encoded
                let encoded = raw.rsplit('\'').next().unwrap_or(raw);
                let decoded = percent_encoding::percent_decode_str(encoded)
                    .decode_utf8()
                    .ok()?;
                if !decoded.is_empty() {
                    return Some(decoded.into_owned());
                }
            }
            "filename" => {
                let name = raw.trim_matches('"').trim();
                if !name.is_empty() {
                    plain = Some(name.to_owned());
                }
            }
            _ => {}
        }
    }
    plain
}

impl<C: BatchConnectionTrait + Send + Sync> Resources<'_, C> {
    /// Ask the host's fetch policy about `url`, resolving its host first so
    /// the policy sees where the connection would go.
    async fn authorise(&self, url: &url::Url) -> Result<(), CapabilityError> {
        let resolved = resolve_host(url).await?;
        match self.core.fetch_policy().decide(url, &resolved) {
            crate::FetchDecision::Allow => Ok(()),
            crate::FetchDecision::Deny(reason) => Err(resource_error(
                CapabilityErrorKind::Unsupported,
                format!(
                    "URL resource refused by the fetch policy ({reason}): {}",
                    url.host_str().unwrap_or("no host")
                ),
            )),
        }
    }

    /// GET `raw` as an anonymous client: no credential, no provider rewrite
    /// rules, no observation. Every hop, including the first, passes the
    /// policy before a connection is made. The whole exchange is bounded by
    /// the operation deadline and the body by the read limit.
    async fn fetch_url(&self, raw: &str) -> Result<ResourceRead, CapabilityError> {
        let url = parse_url(raw)?;
        crate::rt::timeout(self.limits.operation_total, self.fetch_hops(url))
            .await
            .unwrap_or_else(|| Err(transport("URL resource fetch timed out")))
    }

    async fn fetch_hops(&self, mut url: url::Url) -> Result<ResourceRead, CapabilityError> {
        use gproxy_client::OutboundClient;
        let limit = self.codec.max_body_bytes.min(self.limits.read_bytes);
        let mut hops = 0;
        loop {
            self.authorise(&url).await?;
            let client = self
                .core
                .clients()
                .get(&gproxy_client::ConnectionConfig::default())
                .await
                .map_err(|e| transport(format!("outbound client unavailable: {e}")))?;
            let request = http::Request::builder()
                .method(http::Method::GET)
                .uri(url.as_str())
                .header(http::header::ACCEPT, "*/*")
                .body(HttpBody::Bytes(Bytes::new()))
                .map_err(|e| invalid(format!("URL resource request: {e}")))?;
            let response = client.send(request).await?;
            if response.status.is_redirection() {
                let location = response
                    .headers
                    .get(http::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| rejected(response.status))?;
                if hops == MAX_REDIRECT_HOPS {
                    return Err(transport(format!(
                        "URL resource redirected more than {MAX_REDIRECT_HOPS} times"
                    )));
                }
                hops += 1;
                url = url.join(location).map_err(|e| {
                    resource_error(
                        CapabilityErrorKind::Invalid,
                        format!("URL resource redirect target is not a valid URL: {e}"),
                    )
                })?;
                continue;
            }
            if !response.status.is_success() {
                return Err(rejected(response.status));
            }
            let declared = response
                .headers
                .get(http::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            if declared.is_some_and(|length| length > limit) {
                return Err(CapabilityError::new(
                    CapabilityErrorKind::Limit,
                    CapabilityErrorStage::Start,
                    "URL resource declares a body beyond the read limit",
                ));
            }
            let mut read_limits = self.codec;
            read_limits.max_body_bytes = limit;
            let bytes = gproxy_protocol::codec::read_http_body(response.body, read_limits)
                .await
                .map_err(|error| {
                    let kind = match error.kind() {
                        gproxy_protocol::codec::CodecErrorKind::Limit => CapabilityErrorKind::Limit,
                        _ => CapabilityErrorKind::Transport,
                    };
                    CapabilityError::with_source(
                        kind,
                        CapabilityErrorStage::BodyTransfer,
                        "URL resource body could not be read",
                        error,
                    )
                })?;
            return Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: mime_of(&response.headers),
                    length: Some(bytes.len() as u64),
                    filename: filename_of(&response.headers),
                    expires_at: None,
                },
                body: HttpBody::Bytes(bytes),
            });
        }
    }
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
            let id = match reference {
                ResourceReference::Id(id) => id,
                ResourceReference::Url(url) => {
                    let url = parse_url(url)?;
                    self.authorise(&url).await?;
                    return Ok(ResourceMetadata {
                        mime: None,
                        length: None,
                        filename: None,
                        expires_at: None,
                    });
                }
                #[allow(unreachable_patterns)]
                _ => return Err(unknown_reference()),
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
            let id = match reference {
                ResourceReference::Id(id) => id,
                ResourceReference::Url(url) => return self.fetch_url(url).await,
                #[allow(unreachable_patterns)]
                _ => return Err(unknown_reference()),
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
                                | (ResourceReference::Url(_), PublicationKind::Url)
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
            let link_builder = match kind {
                PublicationKind::Id => None,
                PublicationKind::Url => Some(self.core.publication_url().ok_or_else(|| {
                    resource_error(
                        CapabilityErrorKind::Unsupported,
                        "core publishes no URLs: host configured no PublicationUrl",
                    )
                })?),
                #[allow(unreachable_patterns)]
                _ => {
                    return Err(resource_error(
                        CapabilityErrorKind::Unsupported,
                        "unknown publication kind",
                    ));
                }
            };
            let backend = self.backend()?;
            let Some(expires_at) = metadata.expires_at.filter(|at| time_to_ms(*at) > now) else {
                return Err(resource_error(
                    CapabilityErrorKind::Invalid,
                    "a publication needs an expiry in the future",
                ));
            };
            let expires_at_ms = time_to_ms(expires_at);
            let binding_id = crate::ids::random_id();
            // The link is built for the id about to be recorded, before any
            // write: a host refusal leaves no row and no object behind.
            let url = match link_builder {
                None => None,
                Some(builder) => Some(
                    builder
                        .url_for(&crate::PublicationRef {
                            id: &binding_id,
                            scope: &column,
                            mime: metadata.mime.as_deref(),
                            expires_at_ms: Some(expires_at_ms),
                        })
                        .filter(|url| !url.trim().is_empty())
                        .ok_or_else(|| {
                            resource_error(
                                CapabilityErrorKind::Unsupported,
                                "host cannot expose this publication as a URL",
                            )
                        })?,
                ),
            };
            let mut summary = PublicationSummary {
                state: PublicationState::Pending,
                mime: metadata.mime.clone(),
                filename: metadata.filename.clone(),
                length: metadata.length,
                url,
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
                reference: match summary.url {
                    Some(url) => ResourceReference::Url(url),
                    None => ResourceReference::Id(binding_id),
                },
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

/// A capability failure surfaced through the engine API; the transform
/// mapping keeps the kind and message. File backend failures do not go this
/// way: the `Core` methods below return them as `CoreError::File`.
fn core_error(error: CapabilityError) -> crate::CoreError {
    crate::CoreError::Transform(error.into())
}

/// Publications by binding id for the host's download route.
impl<C: BatchConnectionTrait + Send + Sync> Core<C> {
    /// The row behind `id` when it is a publication whose body is stored and
    /// whose expiry has not passed, regardless of scope: the host's route has
    /// already authenticated whoever holds the link.
    async fn live_publication(
        &self,
        id: &str,
        now: i64,
    ) -> Result<Option<(BindingRow, PublicationSummary)>, crate::CoreError> {
        let row = self
            .store()
            .resource_bindings()
            .get_many(&[id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .filter(|row| row.kind == PUBLICATION_KIND);
        let Some(row) = row else {
            return Ok(None);
        };
        let summary = summary_of(&row).map_err(core_error)?;
        if summary.state != PublicationState::Published || row_expired(&row, now) {
            return Ok(None);
        }
        Ok(Some((row, summary)))
    }

    /// The publication behind `id`: its metadata and stored bytes. `None` when
    /// the id names no published body, the body was released, or its expiry
    /// has passed. Needs file storage; the scope is not checked.
    pub async fn read_publication(
        &self,
        id: &str,
    ) -> Result<Option<crate::Publication>, crate::CoreError> {
        let Some((row, summary)) = self.live_publication(id, now_ms()).await? else {
            return Ok(None);
        };
        let backend = self.file_storage().ok_or_else(|| {
            core_error(resource_error(
                CapabilityErrorKind::Unsupported,
                "no file storage configured for locally published resources",
            ))
        })?;
        let bytes = match backend.read(&object_key(&row.id)).await {
            Ok(buffer) => buffer.to_bytes(),
            Err(error) if error.kind() == gproxy_file::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(crate::CoreError::File(error)),
        };
        Ok(Some(crate::Publication {
            id: row.id.clone(),
            scope: row.scope.clone(),
            metadata: metadata_of(&row, &summary),
            body: HttpBody::Bytes(bytes),
        }))
    }

    /// Tombstone the publication behind `id` and delete its stored body, the
    /// way `ResourceAccess::release` does but without a scope. `Ok(false)`
    /// when nothing is published under that id.
    pub async fn delete_publication(&self, id: &str) -> Result<bool, crate::CoreError> {
        let now = now_ms();
        let Some((row, mut summary)) = self.live_publication(id, now).await? else {
            return Ok(false);
        };
        summary.state = PublicationState::Expired;
        let value = serde_json::to_value(&summary).map_err(|e| {
            core_error(resource_error(
                CapabilityErrorKind::Storage,
                format!("summary encode: {e}"),
            ))
        })?;
        self.store()
            .resource_bindings()
            .update_many(vec![resource_binding::ActiveModel {
                id: Set(row.id.clone()),
                summary: Set(value),
                updated_at_ms: Set(now),
                ..Default::default()
            }])
            .await?;
        if let Some(backend) = self.file_storage()
            && let Err(error) = backend.delete(&object_key(&row.id)).await
            && error.kind() != gproxy_file::ErrorKind::NotFound
        {
            return Err(crate::CoreError::File(error));
        }
        Ok(true)
    }
}
