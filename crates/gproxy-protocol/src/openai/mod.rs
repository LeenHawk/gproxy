//! OpenAI wire models, organized by operation family.

pub mod audio;
pub mod common;
pub mod compact;
pub mod conversation;
pub mod count_tokens;
pub mod embeddings;
pub mod files;
pub mod generate_content;
pub mod images;
pub mod memories;
pub mod models;
pub mod realtime;
pub mod rerank;
pub mod search;
pub mod video;

pub use common::*;
pub use compact::*;
pub use count_tokens::*;
pub use generate_content::*;
pub use models::*;

#[cfg(test)]
mod tests;
