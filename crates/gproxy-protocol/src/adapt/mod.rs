//! Runtime-neutral composition over host-supplied capabilities.
//!
//! Targets and authentication remain host-owned. Adapters perform explicit
//! operations and retain their concrete return-mapping state.

pub mod compact;
mod edges;
pub mod embeddings;
pub mod files;
pub mod generate;
pub mod guardian;
pub mod images;
mod json;
pub mod memory;
pub mod models;
pub mod responses_ws;
pub mod video;

pub use edges::{CONVERSION_EDGES, ConversionEdge, conversion_targets, is_conversion_edge};
pub use json::{JsonInvocation, invoke_empty, invoke_json};
