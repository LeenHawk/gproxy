//! Named profiles live in the host/store. This crate only sees effective connection
//! parameters and reuses clients with identical parameters. Requests, authentication,
//! routing, concurrency limits and profile inheritance belong to the caller.
//!
//! Native backends expose their original streaming/request APIs through [`Client`],
//! which also implements the [`OutboundClient`] transport contract that channels
//! and core call. On wasm32 the same [`Client`] and [`ClientPool`] exist, organised
//! by feature: `fetch` drives the JS host's global fetch through web-sys with
//! streaming bodies, `workers` adds Cloudflare Workers WebSocket upgrades, the
//! `reqwest` feature is a buffered HTTP-only fallback, and a host can inject its
//! own transport with `ClientPool::with_client`/`with_factory`. Proxies, TLS
//! emulation and socket pools are native concerns and do not apply there.

// wasm32-unknown-unknown is single-threaded and JS handles are thread-bound;
// shared ownership still goes through Arc so the API is the same on every target.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

mod config;
mod error;
mod outbound;

pub use config::{
    Alpn, Backend, ConnectionConfig, EmulationConfig, Fingerprint, Http2Setting, Http2Settings,
    ProxyConfig, PseudoHeader, RetryPolicy, StreamPriority, TlsVersion,
};
pub use error::Error;
pub use outbound::{ClientBounds, OutboundClient};

mod client;
#[cfg(all(target_arch = "wasm32", feature = "fetch"))]
mod fetch;
#[cfg(feature = "libsql")]
mod libsql;
mod pool;
pub use client::Client;
#[cfg(all(target_arch = "wasm32", feature = "fetch"))]
pub use fetch::FetchClient;
#[cfg(all(target_arch = "wasm32", feature = "workers"))]
pub use fetch::workers::accept as accept_workers_websocket;
#[cfg(target_arch = "wasm32")]
pub use pool::ClientFactory;
pub use pool::ClientPool;

#[cfg(all(feature = "reqwest-native", not(target_arch = "wasm32")))]
pub use reqwest_native;
#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
pub use {reqwest, reqwest_websocket};
#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
pub use {wreq, wreq_util};

/// Names accepted by the compiled wreq backend, using its serialization contract.
/// Builds without native wreq must not advertise browser emulations.
pub fn emulation_profiles() -> Vec<String> {
    #[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
    {
        wreq_util::Profile::VARIANTS
            .iter()
            .rev()
            .map(|profile| {
                serde_json::to_value(profile)
                    .expect("profile serialization")
                    .as_str()
                    .expect("profile name")
                    .to_owned()
            })
            .collect()
    }
    #[cfg(not(all(feature = "wreq", not(target_arch = "wasm32"))))]
    {
        Vec::new()
    }
}
