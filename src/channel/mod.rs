//! Channel implementations and root-owned registry/runtime integration.

#[cfg(any(feature = "channel-aws-bedrock", feature = "channel-kiro"))]
pub(crate) mod aws_eventstream;
pub mod bulletins;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub(crate) mod emulation;
pub mod envelope;
pub mod http_util;
pub(crate) mod metadata;
pub mod oauth;
pub mod realtime_websocket;
pub mod registry;
pub mod resolve;
#[cfg(any(
    feature = "channel-codex",
    feature = "channel-grokbuild",
    feature = "channel-openai"
))]
pub mod responses_websocket;
pub mod settings;
pub mod shaping;

pub use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, Channel, ChannelCatalogEntry,
    ChannelError, ChannelLogin, ChannelMetadata, ChannelSettingField, ChannelSource,
    ChannelStreamDecoder, CookieExchangeCtx, CredentialFamily, DeviceInit, DevicePoll,
    DevicePollCtx, DeviceStartCtx, Disposition, LoginMode, PrepareCtx, PreparedRequest,
    RateLimitResetCreditConsumeOutcome, RateLimitResetCreditConsumeResponse, RateLimitResetCredits,
    RefreshCtx, SettingControl, ShapeCtx, TransportKind, UsageCredits, UsageSnapshot, UsageWindow,
};
pub use gproxy_channel_api::{
    disposition, login, prepared, registration, routes, transport, usage,
};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod registration_tests;
