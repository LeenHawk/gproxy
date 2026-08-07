//! Stable contracts for compile-time GPROXY channel extensions.

pub mod channel;
pub mod context;
pub mod control;
pub mod disposition;
pub mod error;
pub mod login;
pub mod metadata;
pub mod prepared;
pub mod registration;
pub mod routes;
pub mod transport;
pub mod usage;

pub use channel::{Channel, ModelCatalog};
pub use context::{PrepareCtx, RefreshCtx, ShapeCtx, TransportKind};
pub use control::{CredentialControlOperation, CredentialControlResponse};
pub use disposition::Disposition;
pub use error::ChannelError;
pub use gproxy_protocol as protocol;
pub use gproxy_transform as transform;
pub use login::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, ChannelLogin, CookieExchangeCtx,
    DeviceInit, DevicePoll, DevicePollCtx, DeviceStartCtx,
};
pub use metadata::{
    ChannelCatalogEntry, ChannelMetadata, ChannelSettingField, ChannelSource, CredentialFamily,
    LoginMode, SettingControl,
};
pub use prepared::PreparedRequest;
pub use registration::{ChannelRegistration, RegisteredChannel};
pub use transport::ByteStreamDecoder as ChannelStreamDecoder;
pub use usage::{
    RateLimitResetCreditConsumeOutcome, RateLimitResetCreditConsumeResponse, RateLimitResetCredits,
    UsageCredits, UsageSnapshot, UsageWindow,
};
