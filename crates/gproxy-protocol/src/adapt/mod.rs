//! Runtime-neutral composition over host-supplied capabilities.
//!
//! Targets and authentication remain host-owned. Adapters perform explicit
//! operations and retain their concrete return-mapping state.

pub mod compact;
pub mod embeddings;
pub mod files;
pub mod guardian;
pub mod images;
mod json;
pub mod memory;
pub mod models;
pub mod responses_ws;
pub mod video;
pub use json::{JsonInvocation, invoke_empty, invoke_json};

pub mod generate;
