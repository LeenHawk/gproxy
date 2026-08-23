//! Host contract: the services an embedder provides to the core.
//!
//! Decryption is the store implementor's concern — the core receives
//! ready-to-use secret material and never sees a cipher. `gproxy-app`
//! implements envelope encryption inside its store; a bare embedder may
//! store plaintext.
//!
//! Every async method returns the workspace [`BoxFuture`]: object-safe,
//! `Send` on native by construction, no async-fn-in-trait. Public traits
//! do not use AFIT anywhere in this workspace — the one-box-per-I/O-call
//! cost is noise next to the I/O itself, and it settles the Send-bound
//! question instead of deferring it.

use std::time::Duration;

use gproxy_channel_api::{BindingStore, BoxFuture, MaybeSend, MaybeSync, WsDuplex};

use crate::error::{StoreError, TransportError};
use crate::usage::Settlement;

/// Credential identity — defined at the contract layer (bindings reference
/// it), re-exported here for hosts.
pub use gproxy_channel_api::CredentialId;

/// A credential as the core consumes it: which channel understands it and
/// the decrypted secret material in that channel's JSON shape.
#[derive(Debug, Clone)]
pub struct CredentialRecord {
    pub id: CredentialId,
    /// Channel id, e.g. `"openai"`, `"claudecode"`, `"codex"`.
    pub channel: String,
    /// Decrypted secret in the channel's documented shape (API key, OAuth
    /// token set, service-account JSON, ...).
    pub secret: serde_json::Value,
    /// Monotonic version for compare-and-swap on rotation.
    pub version: u64,
}

/// MANDATORY host service: credential persistence.
pub trait CredentialStore {
    fn load<'a>(&'a self, id: CredentialId) -> BoxFuture<'a, Result<CredentialRecord, StoreError>>;

    /// Persist rotated secret material, atomically, guarded by `version`.
    /// Claude rotates the refresh token on every refresh: losing this write
    /// bricks the credential, which is why the method is not optional and
    /// why a stale `version` must fail rather than overwrite.
    fn persist_rotation<'a>(
        &'a self,
        id: CredentialId,
        secret: serde_json::Value,
        version: u64,
    ) -> BoxFuture<'a, Result<(), StoreError>>;

    /// Best-effort exclusive lease so concurrent requests refresh once.
    /// Returns whether this caller holds the lease.
    fn lease_refresh<'a>(
        &'a self,
        id: CredentialId,
        ttl: Duration,
    ) -> BoxFuture<'a, Result<bool, StoreError>>;
}

/// TTL-aware shared cache: affinity pins, refresh leases, counters.
pub trait CacheBackend {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<Vec<u8>>>;
    fn set<'a>(&'a self, key: &'a str, value: Vec<u8>, ttl: Option<Duration>) -> BoxFuture<'a, ()>;
    fn incr<'a>(&'a self, key: &'a str, by: i64, ttl: Option<Duration>) -> BoxFuture<'a, i64>;
}

/// Settlement output. `gproxy-app` writes usage rows; an embedder may
/// aggregate in memory or drop. Never on the hot path's critical section.
pub trait UsageSink {
    fn record<'a>(&'a self, settlement: &'a Settlement) -> BoxFuture<'a, ()>;
}

/// Wire capture, sibling of [`UsageSink`]: the funnel offers every request
/// and response; the sink decides retention and redaction.
pub trait CaptureSink {
    fn record<'a>(&'a self, capture: &'a Capture) -> BoxFuture<'a, ()>;
}

/// One captured exchange. Redaction happens in the sink, not the funnel —
/// the funnel does not know the host's retention policy.
#[derive(Debug)]
pub struct Capture {
    pub request_id: String,
    pub upstream_url: String,
    pub request_body: bytes::Bytes,
    /// `None` when the transport failed before response headers arrived.
    pub response_status: Option<http::StatusCode>,
    pub response_body: Option<bytes::Bytes>,
}

/// Optional ability to run a future after the response is done. If the
/// host provides one, stream settlement detaches (native servers); if not,
/// it completes inline before the stream closes (edge, and any embedder
/// that wants strict ordering). A host without this capability must keep
/// polling an upstream stream after downstream disconnect so inline
/// settlement can reach EOF; Rust `Drop` cannot await asynchronous sinks.
/// This replaces a SettlePolicy enum: the policy *is* whether this
/// capability exists.
pub trait Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>);
    #[cfg(target_arch = "wasm32")]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()>>>);
}

/// Outbound HTTP and websockets. The trait lives here so the core never
/// depends on a concrete client; `gproxy-upstream` provides the canonical
/// impl (wreq, TLS profiles, proxies) and an embedder may bring its own.
/// Request bodies are buffered `Bytes` (transforms and retries need
/// replay); responses stream.
pub trait UpstreamTransport {
    fn send<'a>(
        &'a self,
        request: http::Request<bytes::Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<crate::boundary::ByteStream>, TransportError>>;

    /// Open the upstream socket for a prepared request with
    /// `websocket: true` (Responses-over-WS, realtime, remote control).
    fn open_websocket<'a>(
        &'a self,
        request: http::Request<bytes::Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>>;
}

/// The aggregate a host hands to [`crate::Core`]. Associated types keep
/// everything statically dispatched; no `dyn` on the hot path.
pub trait Host: MaybeSend + MaybeSync + 'static {
    type Credentials: CredentialStore;
    type Cache: CacheBackend;
    type Transport: UpstreamTransport;
    type Usage: UsageSink;
    type Capture: CaptureSink;

    fn credentials(&self) -> &Self::Credentials;
    fn cache(&self) -> &Self::Cache;
    fn transport(&self) -> &Self::Transport;
    fn usage(&self) -> &Self::Usage;
    fn capture(&self) -> &Self::Capture;
    /// `None` → settle inline at EOF and keep draining after client
    /// disconnect; `Some` → detach settlement.
    fn spawner(&self) -> Option<&dyn Spawner> {
        None
    }
    /// Durable resource → credential bindings for stateful service
    /// surfaces. No default implementation exists on purpose: bindings
    /// must be shared across instances and survive restarts, so an
    /// in-memory fallback would fragment silently in multi-instance
    /// deployments. A host that provides `None` cannot register channels
    /// with surface tables — [`crate::Core::new`] fails loudly instead.
    fn bindings(&self) -> Option<&dyn BindingStore> {
        None
    }
}
