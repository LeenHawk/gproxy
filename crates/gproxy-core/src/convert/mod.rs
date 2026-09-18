//! Routing between passthrough and protocol conversion, native endpoint
//! paths, and the drivers that run protocol's adaptation flows over an
//! attempt-bound upstream.

mod endpoints;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod generate;
mod route;
mod spec;

pub use endpoints::generate_endpoint;
pub use route::{Route, RouteError, route};
pub use spec::{is_websocket, response_framing, spec_for};
