//! GPROXY local token counting.
//!
//! The nested `tokenize` module preserves the pre-split module paths. The root
//! re-exports keep the crate ergonomic for direct users.

pub mod tokenize;

pub use tokenize::*;
