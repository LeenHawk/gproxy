//! Multi-channel LLM provider engine (L2 layer of the gproxy SDK).
//!
//! This crate hosts the multi-channel orchestration: [`GproxyEngine`],
//! [`ProviderStore`], retry / credential affinity, the dispatch consumer
//! that walks each channel's [`gproxy_channel::DispatchTable`], and the
//! backend traits for distributed rate-limit / quota / affinity state.
//!
//! Single-channel primitives (the [`gproxy_channel::Channel`] trait,
//! credentials, request / response types, individual channel
//! implementations, health trackers, billing price types, token counting,
//! dispatch-table types) live in `gproxy-channel`. This crate re-exports
//! those so that existing `gproxy_provider::*` paths keep resolving while
//! the workspace completes the SDK layer refactor.

mod affinity;

/// Backend abstractions and in-memory implementations.
pub mod backend;
pub mod engine;
pub mod retry;
pub mod store;
pub mod transform_dispatch;

pub use backend::memory::{InMemoryAffinity, InMemoryQuota, InMemoryRateLimit};
pub use backend::traits::{AffinityBackend, QuotaBackend, QuotaHold, RateLimitBackend};
pub use backend::types::{
    BackendError, QuotaBalance, QuotaError, QuotaExhausted, RateLimitExceeded, RateLimitWindow,
};
pub use engine::{
    ExecuteBody, ExecuteError, ExecuteRequest, ExecuteResult, GproxyEngine, ProviderConfig,
    built_in_model_prices,
};
pub use store::{
    CredentialHealthSnapshot, CredentialSnapshot, CredentialUpdate, EngineEvent, EngineEventSource,
    OAuthFinishResult, ProviderMutator, ProviderRegistry, ProviderSnapshot, ProviderStore,
    ProviderStoreBuilder,
};

// Re-export the single-channel layer so existing `gproxy_provider::*`
// paths keep working during the migration.
pub use gproxy_channel::{
    Channel, ChannelCredential, ChannelRegistration, ChannelRegistry, ChannelSettings,
    CredentialHealth, DispatchRuleDocument, DispatchTable, DispatchTableDocument,
    DispatchTableError, FailedUpstreamAttempt, ModelCooldownHealth, ModelPrice, ModelPriceTier,
    OAuthFlow, PreparedRequest, ProviderDefinition, ResponseClassification,
    RetryableUpstreamResponse, RouteImplementation, RouteKey, UpstreamBodyStream, UpstreamError,
    UpstreamRequestMeta, UpstreamResponse, UpstreamStreamingResponse, Usage,
    is_file_operation, is_file_operation_path,
};

// Backward-compat module aliases so that `use gproxy_provider::channel::Channel`
// (and friends) keep resolving to the moved gproxy-channel types.
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
