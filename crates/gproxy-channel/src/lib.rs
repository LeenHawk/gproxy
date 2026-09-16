//! Standalone channel contracts and explicitly bound operation invocation.
//!
//! Callers supply a provider, credential and outbound client. This crate does not
//! depend on core, store or a concrete client backend. Optional channel abilities
//! use independent traits; concrete channels will be enabled by individual features.
//! No concrete channels are enabled by default.
//! Each protocol operation has its own overridable asynchronous method. HTTP
//! methods preserve streaming bodies; WebSocket methods return duplex connections.

pub mod channel;
pub mod client;

pub use channel::{BaseChannel, ChannelBinding, ChannelError, OperationContext, OperationFuture};
pub use client::OutboundClient;
