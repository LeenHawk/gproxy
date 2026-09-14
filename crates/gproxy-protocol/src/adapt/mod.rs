//! Runtime-neutral composition over host-supplied capabilities.
//!
//! Targets and authentication remain host-owned. Adapters perform explicit
//! operations and retain their concrete return-mapping state.

pub mod compact;
pub mod embeddings;
pub mod files;
pub mod guardian;
mod json;
pub mod images;
pub mod memory;
pub mod models;
pub use json::{JsonInvocation, invoke_empty, invoke_json};

pub mod responses_ws;

pub mod video;

pub mod generate;
