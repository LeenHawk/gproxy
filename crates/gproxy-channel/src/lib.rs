//! Standalone channel contracts and explicitly bound operation invocation.
//!
//! Callers supply a provider, credential and outbound client. This crate depends on
//! gproxy-client only for the `OutboundClient` transport contract and never selects
//! a backend; it does not depend on core or store. Optional channel abilities
//! use independent traits; concrete channels will be enabled by individual features.
//! No concrete channels are enabled by default.
//! Each protocol operation has its own overridable asynchronous method. HTTP
//! methods preserve streaming bodies; WebSocket methods return duplex connections.

pub mod channel;

pub use channel::{
    BaseChannel, ChannelBinding, ChannelError, ChannelRegistry, OperationContext, OperationFuture,
    RegistryError,
};
pub use gproxy_client::{ClientBounds, OutboundClient};
