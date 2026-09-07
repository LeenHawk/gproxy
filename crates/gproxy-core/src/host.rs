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

use gproxy_channel_api::{
    BindingStore, BoxFuture, CallerIdentity, MaybeSend, MaybeSync, UsageView, WsDuplex,
};
use gproxy_protocol::OperationKey;

use crate::boundary::RequestCtx;
use crate::continuation::ContinuationStore;
use crate::control::{Plan, ProviderRef};
use crate::error::CoreError;
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
    /// Acquisition method such as `api_key`, `oauth`, or `cookie`.
    pub kind: String,
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
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, StoreError>>;
    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), StoreError>>;
    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), StoreError>>;
    /// Atomically add `by`, returning the new value. When the key is absent,
    /// it starts at zero and `ttl` establishes its expiry. Incrementing an
    /// existing key never changes its current expiry.
    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, StoreError>>;
    /// Atomically adjust one counter and replace one state value. Either both
    /// writes commit or neither does; quota reconciliation relies on this.
    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected_state: Vec<u8>,
        state: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, StoreError>>;
    /// Compare one opaque value and atomically replace or delete it.
    /// Long-lived ownership leases use the token to prevent stale renewals.
    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, StoreError>>;
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
    /// `None` when no provider request was made (local synthesis).
    pub provider_id: Option<i64>,
    /// `None` when no upstream credential was selected.
    pub credential_id: Option<CredentialId>,
    /// `None` for a locally synthesized response.
    pub upstream_url: Option<String>,
    /// `None` when an orchestrated exchange cannot expose one HTTP request.
    pub request_method: Option<http::Method>,
    pub request_headers: Option<http::HeaderMap>,
    pub request_body: bytes::Bytes,
    /// `None` when the transport failed before response headers arrived.
    pub response_status: Option<http::StatusCode>,
    pub response_headers: Option<http::HeaderMap>,
    pub response_body: Option<bytes::Bytes>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialHealth {
    Healthy,
    Degraded,
    Dead,
}

/// Optional ability to run a future after the response is done. If the
/// host provides one, stream settlement detaches (native servers); if not,
/// it completes inline before the stream closes (edge, and any embedder
/// that wants strict ordering). A host without this capability must keep
/// polling an upstream stream after downstream disconnect, and explicitly
/// close a bridged websocket, so inline settlement can finish; Rust `Drop`
/// cannot await asynchronous sinks.
/// This replaces a SettlePolicy enum: the policy *is* whether this
/// capability exists.
pub trait Spawner: MaybeSync {
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>);
    #[cfg(target_arch = "wasm32")]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()>>>);

    /// Reserve room in the settlement backlog before a settlement is
    /// detached. Settlement is slower than serving, so an unbounded backlog
    /// grows without limit under load; the host bounds it and this future
    /// is the backpressure the response path feels. A stream reserves when
    /// it opens, because its settlement is spawned from `Drop`, which
    /// cannot wait.
    fn reserve_settlement(&self) -> BoxFuture<'_, SettlementPermit>;
}

/// Opaque room in a host's settlement backlog; dropping it releases the slot.
#[cfg(not(target_arch = "wasm32"))]
pub type SettlementPermit = Box<dyn std::any::Any + Send>;
#[cfg(target_arch = "wasm32")]
pub type SettlementPermit = Box<dyn std::any::Any>;

/// Outbound HTTP and websockets. The trait lives here so the core never
/// depends on a concrete client; `gproxy-upstream` provides the canonical
/// impl (wreq, TLS profiles, proxies) and an embedder may bring its own.
/// Request bodies are buffered `Bytes` (transforms and retries need
/// replay); responses stream.
pub trait UpstreamTransport: MaybeSync {
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
    /// Authenticate the gateway caller at the normalized request boundary.
    fn authenticate<'a>(
        &'a self,
        request: &'a RequestCtx,
    ) -> BoxFuture<'a, Result<CallerIdentity, CoreError>>;
    /// Apply permissions, rate limits, and quota pre-charge. `operation` is
    /// `None` for a matched service-surface route. Returning an error must
    /// leave no reservation behind.
    fn admit<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        request: &'a RequestCtx,
        operation: Option<OperationKey>,
        plan: &'a Plan,
    ) -> BoxFuture<'a, Result<(), CoreError>>;
    /// Reconcile a successful exchange or refund a failed admitted request.
    /// `None` means no upstream answer reached settlement.
    fn finish_admission<'a>(
        &'a self,
        request_id: &'a str,
        settlement: Option<&'a Settlement>,
    ) -> BoxFuture<'a, ()>;
    /// Admit one attempt against a credential's own limits. The request id
    /// ties any reservation the host makes to the settlement that releases it.
    fn admit_credential<'a>(
        &'a self,
        request_id: &'a str,
        target: &'a crate::control::Target,
        body: &'a bytes::Bytes,
        settle: gproxy_protocol::SettleMode,
    ) -> BoxFuture<'a, Result<(), CoreError>>;

    /// Reserve additional paid work within an already admitted request.
    fn admit_retry<'a>(
        &'a self,
        request_id: &'a str,
        target: &'a crate::control::Target,
        body: &'a bytes::Bytes,
        settle: gproxy_protocol::SettleMode,
    ) -> BoxFuture<'a, Result<(), CoreError>> {
        self.admit_credential(request_id, target, body, settle)
    }
    /// Count the provider-native request with the host's local tokenizer
    /// ladder. The model and optional map have already been resolved.
    fn count_tokens<'a>(
        &'a self,
        model: &'a str,
        body: &'a bytes::Bytes,
        tokenizer_map: Option<&'a serde_json::Value>,
    ) -> BoxFuture<'a, Result<u64, CoreError>>;

    fn record_credential_health<'a>(
        &'a self,
        credential: CredentialId,
        model: &'a str,
        credential_version: u64,
        health: CredentialHealth,
        response_status: Option<http::StatusCode>,
        detail: &'a str,
    ) -> BoxFuture<'a, ()>;
    /// Sink for upstream quota-window readings observed on responses.
    /// Optional: an embedder without cycle accounting drops them.
    fn begin_credential_usage<'a>(
        &'a self,
        request_id: &'a str,
        target: &'a crate::control::Target,
        started_at_ms: i64,
    ) -> BoxFuture<'a, Result<(), CoreError>> {
        let _ = (request_id, target, started_at_ms);
        Box::pin(async { Ok(()) })
    }

    fn observe_credential_quota<'a>(
        &'a self,
        credential: CredentialId,
        observations: Vec<gproxy_channel_api::QuotaObservation>,
    ) -> BoxFuture<'a, ()> {
        let _ = (credential, observations);
        Box::pin(async {})
    }
    /// Runtime timer used by bounded service-surface polling. Hosts implement
    /// this with their native timer; the core never selects an executor.
    fn wait<'a>(&'a self, duration: Duration) -> BoxFuture<'a, ()>;
    /// Build the caller/provider/selected-credential usage view lent to a synthesizer.
    fn surface_usage<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        provider: &'a ProviderRef,
        credential: CredentialId,
    ) -> Box<dyn UsageView + 'a>;
    /// `None` → settle inline at EOF/WS close and keep pumping after client
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

    /// Persistent OAuth issuer state for public vendor-auth surfaces.
    fn oauth(&self) -> Option<&dyn gproxy_channel_api::OAuthService> {
        None
    }

    /// Process-local ownership of live upstream streams. Channels requiring
    /// this capability are native-only and rejected at startup when absent.
    fn continuations(&self) -> Option<&dyn ContinuationStore> {
        None
    }
}
