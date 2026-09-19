//! Routing between passthrough and protocol conversion, native endpoint
//! paths, and the drivers that run protocol's adaptation flows over an
//! attempt-bound upstream.

mod endpoints;
mod route;
mod spec;

mod call;
mod compact;
mod count_tokens;
mod dispatch;
mod embeddings;
mod files;
pub(crate) mod generate;
mod guardian;
mod images;
mod memory;
mod models;
pub(crate) mod responses_ws;
mod video;

pub(crate) use call::{Call, Converted};
pub(crate) use dispatch::dispatch;
pub use endpoints::generate_endpoint;
pub use route::{Route, RouteError, route};
pub use spec::{is_websocket, response_framing, spec_for};
