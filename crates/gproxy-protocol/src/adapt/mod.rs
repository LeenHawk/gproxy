//! Runtime-neutral composition over host-supplied capabilities.
//!
//! Targets and authentication remain host-owned. Adapters perform explicit
//! operations and retain their concrete return-mapping state.

mod json;
pub use json::{JsonInvocation, invoke_json};
