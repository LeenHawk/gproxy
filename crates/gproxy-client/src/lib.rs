//! Named profiles live in the host/store. This crate only sees effective connection
//! parameters and reuses clients with identical parameters. Requests, authentication,
//! routing, concurrency limits and profile inheritance belong to the caller.
//!
//! Native backends expose their original streaming/request APIs through [`Client`],
//! which also implements the [`OutboundClient`] transport contract that channels
//! and core call. On WASM only configuration types and the contract are available;
//! native proxies and TLS emulation cannot be implemented by browser Fetch.

mod config;
mod error;
mod outbound;

pub use config::{Backend, ConnectionConfig, EmulationConfig, ProxyConfig, RetryPolicy};
pub use error::Error;
pub use outbound::{ClientBounds, OutboundClient};

#[cfg(not(target_arch = "wasm32"))]
mod client;
#[cfg(not(target_arch = "wasm32"))]
mod pool;
#[cfg(not(target_arch = "wasm32"))]
pub use client::Client;
#[cfg(not(target_arch = "wasm32"))]
pub use pool::ClientPool;

#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
pub use {reqwest, reqwest_websocket};
#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
pub use {wreq, wreq_util};
