//! LLM channel/provider engine with multi-channel support.
//!
//! This crate provides a trait-based architecture for routing LLM API requests
//! to upstream providers (OpenAI, Anthropic, Gemini, etc.) with automatic
//! credential rotation, health tracking, and retry logic.
//!
//! # Adding a new channel
//!
//! 1. Create a struct implementing [`Channel`]
//! 2. Implement [`ChannelSettings`] and [`ChannelCredential`] for your config/auth types
//! 3. Implement [`CredentialHealth`] for your health tracking shape
//! 4. Call `inventory::submit!` to register the channel
//!
//! That's it — no other files need to change.

mod affinity;

/// Backend abstractions and in-memory implementations.
pub mod backend;
pub mod billing;
pub mod channel;
pub mod channels;
pub mod count_tokens;
pub mod dispatch;
pub mod engine;
pub mod health;
pub mod http_client;
pub mod provider;
pub mod registry;
pub mod request;
pub mod response;
pub mod retry;
pub mod store;
pub mod suffix;
pub mod transform_dispatch;
pub mod usage;
pub mod utils;

pub use billing::{ModelPrice, ModelPriceTier};
pub use backend::memory::{InMemoryAffinity, InMemoryQuota, InMemoryRateLimit};
pub use backend::traits::{AffinityBackend, QuotaBackend, QuotaHold, RateLimitBackend};
pub use backend::types::{
    BackendError, QuotaBalance, QuotaError, QuotaExhausted, RateLimitExceeded, RateLimitWindow,
};
pub use channel::{Channel, ChannelCredential, ChannelSettings, OAuthFlow};
pub use dispatch::DispatchTable;
pub use engine::{ExecuteBody, ExecuteRequest, ExecuteResult, GproxyEngine, ProviderConfig, Usage};
pub use health::CredentialHealth;
pub use provider::ProviderDefinition;
pub use registry::{ChannelRegistration, ChannelRegistry};
pub use request::PreparedRequest;
pub use response::{
    ResponseClassification, RetryableUpstreamResponse, UpstreamError, UpstreamResponse,
    UpstreamStreamingResponse,
};
pub use store::{
    CredentialHealthSnapshot, CredentialSnapshot, CredentialUpdate, EngineEvent, EngineEventSource,
    OAuthFinishResult, ProviderMutator, ProviderRegistry, ProviderSnapshot, ProviderStore,
    ProviderStoreBuilder,
};
