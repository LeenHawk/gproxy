//! What an engine does with one request once it knows the provider: decide
//! between passthrough and protocol conversion, find the native endpoint, and
//! run protocol's adaptation flows over the capabilities the engine supplies.
//!
//! Stateless. The engine around it owns credentials, storage and transport and
//! hands them in as a [`Host`]; nothing here selects a provider or a
//! credential.

mod call;
mod compact;
mod count_tokens;
mod dispatch;
mod embeddings;
mod endpoints;
mod files;
pub mod form;
pub mod generate;
mod host;
mod ids;
mod images;
mod interactions;
pub mod json_path;
mod memory;
mod models;
pub mod prepare;
pub mod responses_ws;
mod route;
mod spec;
pub mod stream_error;
pub mod surface;
pub mod upstream_error;
mod video;
mod web_search;

pub use call::{Call, ClientRequest, Converted, ModelFacts, STATE_TTL};
pub use dispatch::dispatch;
pub use endpoints::{
    file_content_endpoint, files_endpoint, generate_endpoint, get_model_path, list_models_path,
};
pub use host::{Host, MaybeSend, MaybeSync, Task};
pub use ids::{fill_random, now_ms, random_id};
pub use models::local_metadata as local_model_metadata;
pub use route::{
    Route, RoutingMappings, conversion_targets, default_route, default_route_for_model,
    default_with_native, local_supported, validate_mapping,
};
pub use spec::{is_websocket, response_framing, spec_for};
