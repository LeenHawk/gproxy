//! Routing between passthrough and protocol conversion, native endpoint
//! paths, and the drivers that run protocol's adaptation flows over an
//! attempt-bound upstream.

mod endpoints;
mod route;
mod spec;

#[cfg(not(target_arch = "wasm32"))]
mod call;
#[cfg(not(target_arch = "wasm32"))]
mod compact;
#[cfg(not(target_arch = "wasm32"))]
mod count_tokens;
#[cfg(not(target_arch = "wasm32"))]
mod dispatch;
#[cfg(not(target_arch = "wasm32"))]
mod embeddings;
#[cfg(not(target_arch = "wasm32"))]
mod files;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod generate;
#[cfg(not(target_arch = "wasm32"))]
mod guardian;
#[cfg(not(target_arch = "wasm32"))]
mod images;
#[cfg(not(target_arch = "wasm32"))]
mod memory;
#[cfg(not(target_arch = "wasm32"))]
mod models;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod responses_ws;
#[cfg(not(target_arch = "wasm32"))]
mod video;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use call::{Call, Converted};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use dispatch::dispatch;
pub use endpoints::generate_endpoint;
pub use route::{Route, RouteError, route};
pub use spec::{is_websocket, response_framing, spec_for};
