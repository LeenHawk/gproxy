//! GPROXY v3 embeddable core.
//!
//! Channels, credential lifecycle, protocol transforms, and the execution
//! pipeline, consumable as a library. Hosts (axum server, edge wasm) and
//! other applications embed this crate; it must never depend on an HTTP
//! server framework. Host-provided services (credential persistence, cache,
//! transport, sinks) enter through the traits in [`host`].
//!
//! Interface round 1: boundary types, host contract, control-plane read
//! model, settlement types, and the two execution tiers. Round 2 adds the
//! channel contract and the OperationSpec registry.

// The auto-Send question these warnings point at is a real open item,
// settled in the implementation round (bounds at spawn sites vs. a
// MaybeSend alias). Silencing beats desugaring every draft signature.
#![allow(async_fn_in_trait)]

pub mod api;
pub mod boundary;
pub mod control;
pub mod error;
pub mod host;
pub mod usage;

pub use api::Core;
pub use boundary::{ByteStream, Disposition, ExecOutcome, RequestCtx, ResponseBody, RoutingMode};
pub use control::{ControlPlane, Plan, Pricing, ProviderRef, Target};
pub use error::CoreError;
pub use host::{
    CacheBackend, CaptureSink, CredentialId, CredentialRecord, CredentialStore, Host, Spawner,
    UpstreamTransport, UsageSink,
};
pub use usage::{Ended, NormalizedUsage, Settlement, UsageSource};
