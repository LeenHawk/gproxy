//! Mechanical helpers shared by pairwise transforms.
//!
//! This module must not become a unified provider IR. Keep provider-specific
//! field decisions in the pair module that owns the conversion.

pub mod errors;
pub mod metadata;
pub mod roles;
pub mod sse;
pub mod tools;
pub mod usage;

pub use errors::*;
pub use metadata::*;
pub use roles::*;
pub use sse::*;
pub use tools::*;
pub use usage::*;
