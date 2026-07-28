//! Channel implementations and root-owned registry/runtime integration.

pub(crate) mod aws_eventstream;
pub mod bulletins;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub(crate) mod emulation;
pub mod envelope;
pub mod http_util;
pub mod oauth;
pub mod registry;
pub mod resolve;
pub mod responses_websocket;
pub mod settings;
pub mod shaping;

pub use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, Channel, ChannelError, ChannelLogin,
    ChannelStreamDecoder, CookieExchangeCtx, DeviceInit, DevicePoll, DevicePollCtx, DeviceStartCtx,
    Disposition, PrepareCtx, PreparedRequest, RateLimitResetCreditConsumeOutcome,
    RateLimitResetCreditConsumeResponse, RateLimitResetCredits, RefreshCtx, ShapeCtx,
    TransportKind, UsageCredits, UsageSnapshot, UsageWindow,
};
pub use gproxy_channel_api::{
    disposition, login, prepared, registration, routes, transport, usage,
};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod registration_tests;
