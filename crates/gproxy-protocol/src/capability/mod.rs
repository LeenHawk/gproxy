//! Runtime-neutral capability contracts.
//!
//! Capabilities are deliberately smaller than a client implementation. They do
//! not know about conversion, routing, credentials, persistence engines, or a
//! common content representation. A host supplies the opaque target/scope
//! types and enforces the limits returned by each capability instance.

use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime},
};

use bytes::Bytes;

use crate::{WebSocket, WireRequest, WireResponse, connection::HttpBody};

/// A boxed capability operation future.
///
/// Native futures are `Send`; wasm futures follow the connection stream
/// contracts and do not require `Send`.
#[cfg(not(target_arch = "wasm32"))]
pub type CapabilityFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg(target_arch = "wasm32")]
pub type CapabilityFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// The limits a capability instance is required to enforce.
///
/// Hosts may opt into bounds for a particular instance. `Duration::MAX` disables
/// a timer and integer maxima impose no additional size cap. Explicit finite
/// limits continue to apply after an operation returns a streaming body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityLimits {
    /// Total wall-clock time allowed for one operation, including body transfer.
    pub operation_total: Duration,
    /// Maximum idle interval between stream progress events.
    pub stream_idle: Duration,
    /// Maximum bytes accepted by a read or an upstream response body.
    pub read_bytes: u64,
    /// Maximum bytes accepted by a write or an upstream request body.
    pub write_bytes: u64,
    /// Maximum payload bytes in one WebSocket frame.
    pub ws_frame_bytes: u64,
}

/// A stable category for a capability failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CapabilityErrorKind {
    Unsupported,
    NotFound,
    Expired,
    Conflict,
    Limit,
    Transport,
    Storage,
    Invalid,
}

/// Whether an error occurred before or during transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CapabilityErrorStage {
    Start,
    BodyTransfer,
    Stream,
}

/// A structured capability error that retains the originating error when one
/// exists. HTTP status responses are not represented by this type: an upstream
/// HTTP non-2xx response is still a successful `send` result and retains its
/// complete response body.
#[derive(Debug)]
pub struct CapabilityError {
    kind: CapabilityErrorKind,
    stage: CapabilityErrorStage,
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl CapabilityError {
    /// Creates an error without a lower-level source.
    pub fn new(
        kind: CapabilityErrorKind,
        stage: CapabilityErrorStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            stage,
            message: message.into(),
            source: None,
        }
    }

    /// Creates an error while retaining the lower-level source.
    pub fn with_source(
        kind: CapabilityErrorKind,
        stage: CapabilityErrorStage,
        message: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            kind,
            stage,
            message: message.into(),
            source: Some(source.into()),
        }
    }

    pub fn kind(&self) -> CapabilityErrorKind {
        self.kind
    }

    pub fn stage(&self) -> CapabilityErrorStage {
        self.stage
    }

    pub fn source_error(&self) -> Option<&(dyn Error + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:?}/{:?})", self.message, self.kind, self.stage)
    }
}

impl Error for CapabilityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

/// The result of a WebSocket-capable upstream connect attempt.
pub enum UpstreamConnection {
    /// The handshake succeeded and the socket is ready for frames.
    Connected {
        handshake: WireResponse<()>,
        socket: WebSocket,
    },
    /// The upstream rejected the handshake. The complete response body is
    /// retained so the host can decode or relay the vendor error.
    Rejected(WireResponse<HttpBody>),
}

impl fmt::Debug for UpstreamConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connected { handshake, .. } => f
                .debug_struct("Connected")
                .field("handshake", handshake)
                .field("socket", &"WebSocket { .. }")
                .finish(),
            Self::Rejected(response) => f.debug_tuple("Rejected").field(response).finish(),
        }
    }
}

/// The upstream transport family.
pub trait Upstream {
    /// Opaque host-owned destination identity, origin, and authentication.
    type Target;

    /// Sends one already-encoded request. A non-2xx HTTP response is returned
    /// as `Ok(WireResponse)` and is never converted into a capability error.
    /// Hosts must validate that the request path is origin-relative and
    /// compatible with `target`, and must replace or remove request auth in
    /// both headers and query parameters (such as `key` or `access_token`)
    /// rather than inheriting source credentials. Implementations never route or
    /// recursively invoke conversion.
    fn send<'a>(
        &'a self,
        target: &'a Self::Target,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>>;

    /// Performs an HTTP upgrade. A rejected handshake is a response result,
    /// including its full body, while transport failures are errors. The same
    /// origin-relative path, target compatibility, and source-auth replacement
    /// rules as [`Self::send`] apply.
    fn connect<'a>(
        &'a self,
        target: &'a Self::Target,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>>;

    /// Limits bound to this capability instance.
    fn limits(&self) -> CapabilityLimits;
}

/// File-only metadata exposed by resource access. A filename is retained across
/// upload and read so adapters can preserve the source file's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceMetadata {
    pub mime: Option<String>,
    pub length: Option<u64>,
    pub filename: Option<String>,
    pub expires_at: Option<SystemTime>,
}

/// A resource locator understood by the host boundary. An `Id` is interpreted
/// using the associated scope's upstream and identity; a `Url` remains subject
/// to host authorization. Neither form grants deletion authority.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResourceReference {
    Id(String),
    Url(String),
}

/// The representation requested for a newly published resource. If the host
/// cannot provide the requested form, it must reject before consuming the body
/// or causing any remote side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum PublicationKind {
    Id,
    Url,
}

/// A resource body and its metadata.
pub struct ResourceRead {
    pub metadata: ResourceMetadata,
    pub body: HttpBody,
}

impl fmt::Debug for ResourceRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourceRead")
            .field("metadata", &self.metadata)
            .field("body", &self.body)
            .finish()
    }
}

/// The handle and reference returned by a successful publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedResource<H> {
    pub handle: H,
    pub reference: ResourceReference,
    pub metadata: ResourceMetadata,
}

/// The durable status of an idempotent publication operation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum PublicationStatus<H> {
    Missing,
    Pending,
    Published(PublishedResource<H>),
    Expired,
}

/// Scoped resource publication and access.
pub trait ResourceAccess {
    type Scope;
    type PublishedHandle;

    /// Returns file metadata or `NotFound`/`Expired` as appropriate.
    fn resolve<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>>;

    /// Reads a resource as a streamable body with file-only metadata.
    fn read<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>>;

    /// Publishes one body under a host-assigned reference and handle.
    ///
    /// `operation_id` is idempotency-scoped by `scope`. Hosts check and update
    /// its stored state atomically, including stored expiry, before validating
    /// new metadata. No preceding status query is required.
    ///
    /// A duplicate completed operation with the same `kind` returns the original
    /// publication and metadata without consuming the new body. A different
    /// kind or a pending operation returns `Conflict`; a released or expired
    /// operation returns `Expired`, irrespective of newly supplied metadata.
    /// An existing publication's metadata is never overwritten by a retry.
    /// Callers can inspect the durable state with
    /// [`Self::publication_status`]. If the caller is dropped after remote
    /// publication but before receiving the handle, it can recover the result
    /// with [`Self::publication_status`] without replaying the body. The
    /// metadata expiry for a new operation must be present and in the future;
    /// otherwise the host returns `Invalid` before consuming the body. If `kind`
    /// is unsupported, the host returns `Unsupported` before any side effect.
    /// Rejection and duplicate paths do not consume the supplied body.
    fn publish<'a>(
        &'a self,
        scope: &'a Self::Scope,
        operation_id: &'a str,
        kind: PublicationKind,
        metadata: ResourceMetadata,
        body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<Self::PublishedHandle>, CapabilityError>>;

    /// Recovers publication state after cancellation or a lost result.
    fn publication_status<'a>(
        &'a self,
        scope: &'a Self::Scope,
        operation_id: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<Self::PublishedHandle>, CapabilityError>>;

    /// Releases a published handle after the host verifies signed ownership and
    /// scope. This is intentionally separate from publication and does not add
    /// a delete-existing-resource operation for arbitrary references.
    /// On success the operation becomes `Expired`, retaining its idempotency
    /// record: status queries and duplicate publishes must not return a stale
    /// publication or recreate the resource. Expiry has the same terminal effect.
    fn release<'a>(
        &'a self,
        scope: &'a Self::Scope,
        handle: &'a Self::PublishedHandle,
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>>;

    fn limits(&self) -> CapabilityLimits;
}

/// An opaque, host-generated state version. Versions must never be reused,
/// including after delete and recreate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(Vec<u8>);

impl Version {
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// A state entry returned by a store read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEntry {
    pub payload: Bytes,
    pub version: Version,
    pub expires_at: Option<SystemTime>,
}

/// Replacement data for a state CAS. `None` replacement means delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateWrite {
    pub payload: Bytes,
    pub expires_at: Option<SystemTime>,
}

/// The outcome of a scoped compare-and-exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CasResult {
    /// The write was applied. A replacement has a fresh version; deletion has
    /// no resulting entry and therefore returns `None`.
    Applied(Option<Version>),
    /// The expected version did not match. No current entry is disclosed.
    Conflict,
}

/// An atomically scoped state store.
pub trait StateStore {
    type Scope;

    /// Expired entries are treated as absent, consistently for reads and CAS.
    fn get<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>>;

    /// `expected = None` means absent. The operation is atomic within `scope`;
    /// a conflict never returns the current entry. Hosts generate versions and
    /// never reuse them across delete/recreate cycles.
    fn compare_exchange<'a>(
        &'a self,
        scope: &'a Self::Scope,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>>;

    fn limits(&self) -> CapabilityLimits;
}
