//! Gemini wire model types.

pub mod batch;
pub mod caching;
pub mod common;
pub mod count_tokens;
pub mod embeddings;
mod extensible;
pub mod files;
pub mod generate_content;
pub mod images;
pub mod models;
pub mod video;

pub use batch::*;
pub use caching::*;
pub use common::*;
pub use count_tokens::*;
pub use embeddings::*;
pub use files::*;
pub use generate_content::*;
pub use images::*;
pub use models::*;
pub use video::*;

#[cfg(test)]
mod tests;
