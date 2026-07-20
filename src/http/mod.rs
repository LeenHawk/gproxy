//! HTTP surface, split by direction:
//! - [`client`] — outbound transport to upstreams (shared; wreq native / fetch edge)
//! - [`server`] — inbound axum router + handlers (shared; native serve + wasm edge)
//! - [`edge`] — inbound WinterCG `fetch` entry (wasm)

pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod cors;
pub(crate) mod egress;
pub(crate) mod ops;
pub(crate) mod responses_ws;
pub mod server;

// Cross-target admin/portal business implementation and route dispatcher.
// Native axum and the wasm edge entry are both framework adapters around it.
pub mod admin_api;

// The edge entry wires all edge backends together (runtime-selected), so it
// requires the full edge feature bundle; build with `--features edge`.
#[cfg(all(
    target_arch = "wasm32",
    feature = "persist-libsql",
    feature = "cache-libsql",
    feature = "cache-upstash",
    feature = "upstream-fetch"
))]
pub mod edge;
