//! GPROXY v3 embeddable core.
//!
//! Channels, credential lifecycle, protocol transforms, and the execution
//! pipeline, consumable as a library. Hosts (axum server, edge wasm) and
//! other applications embed this crate; it must never depend on an HTTP
//! server framework. Host-provided services (credential persistence, cache,
//! transport, sinks) enter through the traits in [`host`].
//!
//! Interface rounds 1–3 are drafted: boundary types, host contract (all
//! async methods return the workspace [`BoxFuture`] — no AFIT in public
//! traits), control-plane read model, settlement types, the two execution
//! tiers, and (via `gproxy-channel-api`) the channel contract with surface
//! hooks.

pub mod api;
pub mod boundary;
pub mod control;
pub mod error;
pub mod host;
pub mod usage;

mod attempt;
mod execution;
mod funnel;
mod surface;

#[cfg(test)]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Shared<T> = std::sync::Arc<T>;
#[cfg(target_arch = "wasm32")]
pub(crate) type Shared<T> = std::rc::Rc<T>;

pub use gproxy_channel_api as channel_api;
pub use gproxy_channel_api::{
    BindingStore, BoxFuture, CallerIdentity, QuotaWindow, UsageView, WsDuplex, WsFrame,
};
pub use gproxy_channels as channels;
pub use gproxy_protocol as protocol;
pub use gproxy_protocol::OperationKey;

pub use api::{Core, InitError};
pub use boundary::{ByteStream, Disposition, ExecOutcome, RequestCtx, ResponseBody, RoutingMode};
pub use control::{ControlPlane, Plan, Pricing, ProviderRef, Target};
pub use error::CoreError;
pub use host::{
    CacheBackend, CaptureSink, CredentialId, CredentialRecord, CredentialStore, Host, Spawner,
    UpstreamTransport, UsageSink,
};
pub use usage::{Ended, NormalizedUsage, Settlement, UsageSource};
