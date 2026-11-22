//! Standalone channel contracts and explicitly bound operation invocation.
//!
//! Callers supply a provider, credential and outbound client. The account side
//! (views, sign-in, refresh, state) is `gproxy-upstream`; this crate adds the
//! model side. It depends on gproxy-client only for the `OutboundClient`
//! transport contract and never selects a backend; it does not depend on core or store. Optional channel abilities
//! use independent traits; concrete channels will be enabled by individual features.
//! No concrete channels are enabled by default.
//! Each protocol operation has its own overridable asynchronous method. HTTP
//! methods preserve streaming bodies; WebSocket methods return duplex connections.

pub mod channel;
/// Concrete channels, each behind its own Cargo feature.
pub mod channels;

pub use channel::{
    BaseChannel, ChannelBinding, ChannelCapabilities, ChannelDescriptor, ChannelRegistry,
    HOST_CONFIG_KEYS, OperationContext,
};
pub use gproxy_client::{ClientBounds, OutboundClient};
