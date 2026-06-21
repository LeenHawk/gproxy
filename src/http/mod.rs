//! HTTP surface, split by direction:
//! - [`client`] — outbound transport to upstreams (shared; wreq native / fetch edge)
//! - [`server`] — inbound axum router + handlers (shared; native serve + wasm edge)
//! - [`edge`] — inbound WinterCG `fetch` entry (wasm)

pub mod client;
pub mod server;

// Cross-target admin/portal dispatcher: compiled into the wasm edge worker (it
// is called from `edge/mod.rs::fetch`) and into native test builds (so the
// dispatcher can be driven by native integration tests). Skipped in native
// release — the native server has its own axum router.
#[cfg(any(target_arch = "wasm32", test))]
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
