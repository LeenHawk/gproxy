//! GPROXY provider protocol transforms.
//!
//! The nested `transform` module preserves the pre-split module paths. The root
//! re-exports keep the crate ergonomic for direct users.

pub use gproxy_protocol as protocol;

pub mod transform;

pub use transform::*;
