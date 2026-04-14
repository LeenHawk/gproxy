//! Multi-channel LLM orchestration engine (L2 layer of the gproxy SDK).
//!
//! This crate hosts `GproxyEngine`, `ProviderStore`, the retry loop,
//! credential affinity, the dispatch consumer, backend traits for
//! distributed rate-limit / quota / affinity state, and framework-independent
//! routing helpers (classify, permission, rate_limit, provider_prefix,
//! model_alias, model_extraction, header sanitize).
//!
//! The single-channel layer (`Channel` trait + concrete channels + credentials
//! + request/response types) lives in `gproxy-channel`. Wire-format types and
//! protocol transforms live in `gproxy-protocol`.

mod affinity;

/// Backend abstractions and in-memory implementations.
pub mod backend;
pub mod engine;
pub mod retry;
pub mod routing;
pub mod store;

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
