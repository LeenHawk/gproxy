//! gproxy SDK — layered facade over `gproxy-protocol`, `gproxy-channel`, and
//! `gproxy-engine`.
//!
//! Most users want `engine` (the multi-channel engine) or `channel` (a single
//! upstream with credentials + typed requests). `protocol` is exposed for
//! users who only need wire-format types and cross-protocol transforms.
//!
//! The older `provider` and `routing` module aliases are preserved for
//! backward compatibility with code written before the SDK layer split.
//! They now re-export directly from the new crates instead of going
//! through the deleted `gproxy-provider` / `gproxy-routing` wrapper crates.
//! New code should prefer `channel` / `engine` directly.

pub use gproxy_channel as channel;
pub use gproxy_engine as engine;
pub use gproxy_protocol as protocol;

/// **Deprecated** — use [`channel`] and [`engine`] instead.
///
/// Pre-refactor, this alias pointed at the (now deleted) `gproxy-provider`
/// crate which bundled single-channel and multi-channel layers together.
/// Today it is an inline re-export of `gproxy-channel::*` plus the
/// multi-channel surface of `gproxy-engine`, preserved so existing
/// `gproxy_sdk::provider::X` imports keep resolving during the migration.
pub mod provider {
    pub use gproxy_channel::*;
    pub use gproxy_engine::{
        AffinityBackend, BackendError, ExecuteBody, ExecuteError, ExecuteRequest, ExecuteResult,
        GproxyEngine, InMemoryAffinity, InMemoryQuota, InMemoryRateLimit, ProviderConfig,
        QuotaBackend, QuotaBalance, QuotaError, QuotaExhausted, QuotaHold, RateLimitBackend,
        RateLimitExceeded, RateLimitWindow, built_in_model_prices,
    };
    pub use gproxy_engine::{
        CredentialHealthSnapshot, CredentialSnapshot, CredentialUpdate, EngineEvent,
        EngineEventSource, OAuthFinishResult, ProviderMutator, ProviderRegistry, ProviderSnapshot,
        ProviderStore, ProviderStoreBuilder,
    };
    pub use gproxy_engine::backend;
    pub use gproxy_engine::engine;
    pub use gproxy_engine::retry;
    pub use gproxy_engine::store;
    pub use gproxy_engine::transform_dispatch;
}

/// **Deprecated** — use [`engine::routing`] instead.
///
/// Pre-refactor, this alias pointed at the (now deleted) `gproxy-routing`
/// crate. Today it is an inline re-export of
/// `gproxy_engine::routing::*`, preserved so existing
/// `gproxy_sdk::routing::X` imports keep resolving during the migration.
pub mod routing {
    pub use gproxy_engine::routing::*;
    pub use gproxy_engine::routing::classify;
    pub use gproxy_engine::routing::error;
    pub use gproxy_engine::routing::model_alias;
    pub use gproxy_engine::routing::model_extraction;
    pub use gproxy_engine::routing::permission;
    pub use gproxy_engine::routing::provider_prefix;
    pub use gproxy_engine::routing::rate_limit;
    // Historical name: gproxy-routing had a `sanitize` module for HTTP
    // header/query sanitization. In gproxy-engine it lives at
    // `routing::headers` to avoid shadowing the L1 body-sanitize module.
    pub use gproxy_engine::routing::headers as sanitize;
}
