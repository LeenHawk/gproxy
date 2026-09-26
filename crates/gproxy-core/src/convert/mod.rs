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
mod web_search;

pub(crate) use call::{Call, ClientRequest, Converted};
pub(crate) use dispatch::dispatch;
pub use endpoints::generate_endpoint;
pub use route::{
    Route, RouteError, RoutingMappings, conversion_targets, default_route, default_route_for_model,
    local_supported, resolve_route, resolve_route_for_model, route, route_for_model,
    validate_mapping,
};
pub(crate) mod local;
pub use spec::{is_websocket, response_framing, spec_for};
