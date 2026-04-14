//! Single-channel access layer for gproxy.
//!
//! This crate provides the L1 "channel" layer of the gproxy SDK: the
//! [`Channel`] trait, concrete channel implementations (OpenAI, Anthropic,
//! Gemini, and friends), credential and response types, token counting,
//! billing price types, dispatch table types, and a thin [`executor`]
//! entry point for running a single upstream request without retry or
//! credential rotation.
//!
//! Multi-channel orchestration (engine, store, retry, affinity, routing
//! helpers) lives in `gproxy-engine`. Wire-format types and protocol
//! transforms live in `gproxy-protocol`.
//!
//! # Adding a new channel
//!
//! 1. Create a struct implementing [`Channel`]
//! 2. Implement [`ChannelSettings`] and [`ChannelCredential`] for your
//!    config/auth types
//! 3. Implement [`CredentialHealth`] for your health tracking shape
//! 4. Call `inventory::submit!` to register the channel
//!
//! That's it — no other files need to change.

pub mod billing;
pub mod channel;
pub mod channels;
pub mod count_tokens;
pub mod dispatch;
pub mod executor;
pub mod file_operation;
pub mod health;
pub mod http_client;
pub mod meta;
pub mod provider;
pub mod registry;
pub mod request;
pub mod response;
pub mod usage;
pub mod utils;

pub use billing::{ModelPrice, ModelPriceTier};
pub use channel::{Channel, ChannelCredential, ChannelSettings, OAuthFlow};
pub use dispatch::{
    DispatchRuleDocument, DispatchTable, DispatchTableDocument, DispatchTableError,
    RouteImplementation, RouteKey,
};
pub use executor::{execute_once, execute_once_stream};
pub use file_operation::{is_file_operation, is_file_operation_path};
pub use health::{CredentialHealth, ModelCooldownHealth};
pub use meta::UpstreamRequestMeta;
pub use provider::ProviderDefinition;
pub use registry::{ChannelRegistration, ChannelRegistry};
pub use request::PreparedRequest;
pub use response::{
    FailedUpstreamAttempt, ResponseClassification, RetryableUpstreamResponse, UpstreamBodyStream,
    UpstreamError, UpstreamResponse, UpstreamStreamingResponse,
};
pub use usage::Usage;
