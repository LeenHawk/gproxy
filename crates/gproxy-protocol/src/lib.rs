//! GPROXY protocol types and endpoint metadata.
//!
//! The nested `protocol` module preserves the paths used before this code was
//! split out of the main crate. The root re-exports keep the public API compact
//! for downstream crates.

pub mod protocol;

pub use protocol::*;
