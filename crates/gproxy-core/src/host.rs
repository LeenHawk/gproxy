//! Host contract: the services an embedder provides to the core.
//!
//! Decryption is the store implementor's concern — the core receives
//! ready-to-use secret material and never sees a cipher. `gproxy-app`
//! implements envelope encryption inside its store; a bare embedder may
//! store plaintext. Everything here is async-fn-in-trait; the Send-bound
//! strategy is settled in the implementation round (see lib.rs allow note).

use std::time::Duration;

use crate::error::StoreError;
use crate::usage::Settlement;

/// Stable credential identity. i64 to match relational primary keys;
/// embedders without a database can hand out any distinct values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CredentialId(pub i64);

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
    async fn load(&self, id: CredentialId) -> Result<CredentialRecord, StoreError>;

    /// Persist rotated secret material, atomically, guarded by `version`.
    /// Claude rotates the refresh token on every refresh: losing this write
    /// bricks the credential, which is why the method is not optional and
    /// why a stale `version` must fail rather than overwrite.
    async fn persist_rotation(
        &self,
        id: CredentialId,
        secret: serde_json::Value,
        version: u64,
    ) -> Result<(), StoreError>;

    /// Best-effort exclusive lease so concurrent requests refresh once.
    /// Returns whether this caller holds the lease.
    async fn lease_refresh(&self, id: CredentialId, ttl: Duration) -> Result<bool, StoreError>;
}

/// TTL-aware shared cache: affinity bindings, refresh leases, counters.
pub trait CacheBackend {
    async fn get(&self, key: &str) -> Option<Vec<u8>>;
    async fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>);
    async fn incr(&self, key: &str, by: i64, ttl: Option<Duration>) -> i64;
}

/// Settlement output. `gproxy-app` writes usage rows; an embedder may
/// aggregate in memory or drop. Never on the hot path's critical section.
pub trait UsageSink {
    async fn record(&self, settlement: &Settlement);
}

/// Wire capture, sibling of [`UsageSink`]: the funnel offers every request
/// and response; the sink decides retention and redaction.
pub trait CaptureSink {
    async fn record(&self, capture: &Capture);
}

/// One captured exchange. Redaction happens in the sink, not the funnel —
/// the funnel does not know the host's retention policy.
#[derive(Debug)]
pub struct Capture {
    pub request_id: String,
    pub upstream_url: String,
    pub request_body: bytes::Bytes,
    pub response_status: http::StatusCode,
    pub response_body: Option<bytes::Bytes>,
}

/// Optional ability to run a future after the response is done. If the
/// host provides one, stream settlement detaches (native servers); if not,
/// it completes inline before the stream closes (edge, and any embedder
/// that wants strict ordering). This replaces a SettlePolicy enum: the
/// policy *is* whether this capability exists.
pub trait Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>);
    #[cfg(target_arch = "wasm32")]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()>>>);
}

/// Outbound HTTP. The trait lives here so the core never depends on a
/// concrete client; `gproxy-upstream` provides the canonical impl (wreq,
/// TLS profiles, proxies) and an embedder may bring its own. Request
/// bodies are buffered `Bytes` (transforms and retries need replay);
/// responses stream.
pub trait UpstreamTransport {
    async fn send(
        &self,
        request: http::Request<bytes::Bytes>,
    ) -> Result<http::Response<crate::boundary::ByteStream>, crate::error::TransportError>;
}

/// The aggregate a host hands to [`crate::Core`]. Associated types keep
/// everything statically dispatched; no `dyn` on the hot path.
pub trait Host {
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
    /// `None` → settle inline on stream end; `Some` → detach.
    fn spawner(&self) -> Option<&dyn Spawner> {
        None
    }
}
