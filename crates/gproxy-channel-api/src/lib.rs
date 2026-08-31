//! GPROXY v3 channel contract.
//!
//! A channel owns provider-specific knowledge: upstream URLs, auth
//! injection, request shaping, response classification, stream decoding,
//! usage extraction, OAuth refresh, and its service-surface table. The
//! engine owns everything else — routing, admission, failover, settlement.
//!
//! The [`Channel`](channel::Channel) trait is deliberately synchronous and
//! object-safe: adapters are pure logic over borrowed data, so a registry
//! can hold `Box<dyn Channel>` with no async-trait machinery. The one
//! async concern — credential refresh — returns a boxed future from a
//! plain method.

pub mod channel;
pub mod disposition;
pub mod endpoint;
pub mod login;
pub mod operation;
pub mod registry;
pub mod resource;
pub mod session;
pub mod surface;
pub mod traffic;
pub mod usage;
pub mod wire;

pub use channel::{
    Channel, ChannelDefaultRule, ChannelDefaultRuleSet, ChannelDescriptor, ChannelError,
    ChannelField, ChannelFieldControl, ChannelRouteAction, ChannelSupport, ChannelTrafficPolicy,
    Frame, PrepareCtx, PreparedRequest, ResponseShapeCtx, ResponseView, SimpleHttp, StreamCtx,
    StreamDecoder, StreamEnd, StreamTail, UsageCtx,
};
pub use disposition::Disposition;
pub use endpoint::endpoint_override_key;
pub use login::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, ChannelLogin, ChannelLoginRef,
    CookieExchangeCtx, DeviceInit, DevicePoll, DevicePollCtx, DeviceStartCtx, LoginDescriptor,
    LoginMode, LoginParam, LoginParamKind,
};
pub use operation::{
    DriverInput, OperationDriver, OperationStep, OperationStream, Pause, StepResponse, StreamOutput,
};
pub use registry::ChannelRegistry;
pub use resource::{ResourceCtx, ResourceMutation};
pub use session::{
    PreparedSession, RealtimeMeter, SessionObservation, SessionPrepareCtx, SessionPreparer,
    SessionUsage, SessionUsageKind,
};
pub use surface::{
    Binding, BindingPage, BindingStore, CallerIdentity, ForwardRetry, ForwardSpec, Page,
    ProviderView, QuotaWindow, StateError, SurfaceAction, SurfaceAffinity, SurfaceBody,
    SurfaceEntry, SurfaceInvoke, SurfaceReply, SurfaceRequest, SurfaceServices, SurfaceTable,
    SynthCtx, Synthesizer, UsageView, UsageWindow,
};
pub use traffic::{TrafficBlacklistConfig, TrafficPolicyConfig};
pub use usage::{
    NormalizedUsage, QuotaObservation, QuotaResetCredits, QuotaResetOutcome, QuotaResetResult,
};
pub use wire::{
    Alpn, ByteStream, ClientProfile, ConfiguredClientProfile, CredentialId, Http2Profile,
    Http2Setting, MaybeSend, MaybeSync, PseudoHeader, TlsVersion, TransportError, WsDuplex,
    WsFrame,
};

/// Boxed future with the wasm `Send` split — the one language-level tax
/// this crate carries for the single-threaded wasm target.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + 'a>>;
