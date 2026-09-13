//! Runtime-neutral composition over host-supplied capabilities.
//!
//! Targets and authentication remain host-owned. Adapters perform explicit
//! operations and retain their concrete return-mapping state.

pub mod embeddings;
pub mod files;
mod json;
pub mod models;
pub use json::{JsonInvocation, invoke_empty, invoke_json};
