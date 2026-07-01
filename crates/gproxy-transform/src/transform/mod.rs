//! Provider-to-provider protocol transforms.
//!
//! Protocol modules model provider wire shapes only. This module owns
//! conversion between those wire shapes, organized by operation capability.

pub mod common;
pub mod compact;
mod context;
pub mod count_tokens;
pub mod dispatch;
pub mod embeddings;
mod error;
pub mod generate_content;
pub mod images;
pub mod models;
mod registry;
pub mod routing;
pub mod stream_adapter;

pub use context::*;
pub use error::*;
pub use registry::*;
