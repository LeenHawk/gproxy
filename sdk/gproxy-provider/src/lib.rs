//! **Deprecated aggregator crate** — prefer [`gproxy_channel`] and
//! [`gproxy_engine`] directly.
//!
//! This crate used to own the single-channel layer (Channel trait, concrete
//! channels, request/response types) and the multi-channel engine
//! (GproxyEngine, ProviderStore, retry, affinity, backend traits). As of the
//! SDK layer refactor the content has split into `gproxy-channel` (L1, single
//! channel) and `gproxy-engine` (L2, multi-channel orchestration). This
//! crate keeps working as a pure re-export aggregator so existing
//! `gproxy_provider::*` paths in downstream code and in the `gproxy-sdk`
//! facade continue to resolve.
//!
//! New code should import from `gproxy-channel` / `gproxy-engine`.
//!
//! This crate will be removed entirely in a follow-up commit once every
//! consumer has migrated.

pub use gproxy_channel::*;
pub use gproxy_engine::{
    AffinityBackend, BackendError, ExecuteBody, ExecuteError, ExecuteRequest, ExecuteResult,
    GproxyEngine, InMemoryAffinity, InMemoryQuota, InMemoryRateLimit, ProviderConfig,
    QuotaBackend, QuotaBalance, QuotaError, QuotaExhausted, QuotaHold, RateLimitBackend,
    RateLimitExceeded, RateLimitWindow, built_in_model_prices,
};
pub use gproxy_engine::{
    CredentialHealthSnapshot, CredentialSnapshot, CredentialUpdate, EngineEvent, EngineEventSource,
    OAuthFinishResult, ProviderMutator, ProviderRegistry, ProviderSnapshot, ProviderStore,
    ProviderStoreBuilder,
};

// Backward-compat module aliases so existing `use gproxy_provider::engine::*`,
// `use gproxy_provider::store::*`, etc. keep resolving after the split.
pub use gproxy_engine::backend;
pub use gproxy_engine::engine;
pub use gproxy_engine::retry;
pub use gproxy_engine::store;
pub use gproxy_engine::transform_dispatch;

// Re-export the gproxy-channel module aliases that used to live under
// `gproxy_provider::*`.
pub use gproxy_channel::billing;
pub use gproxy_channel::channel;
pub use gproxy_channel::channels;
pub use gproxy_channel::count_tokens;
pub use gproxy_channel::dispatch;
pub use gproxy_channel::health;
pub use gproxy_channel::http_client;
pub use gproxy_channel::provider;
pub use gproxy_channel::registry;
pub use gproxy_channel::request;
pub use gproxy_channel::response;
pub use gproxy_channel::usage;
pub use gproxy_channel::utils;
