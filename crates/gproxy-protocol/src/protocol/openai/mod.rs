//! OpenAI provider wire models.
//!
//! These modules mirror OpenAI JSON wire shapes only. Provider conversion and
//! routing logic belongs outside this provider model layer.

mod audio;
mod common;
mod compact;
mod conversation;
mod count_tokens;
mod embeddings;
pub mod generate_content;
mod images;
mod models;
pub mod realtime;
mod rerank;
mod video;

pub use audio::*;
pub use common::*;
pub use compact::*;
pub use conversation::*;
pub use count_tokens::*;
pub use embeddings::*;
pub use generate_content::*;
pub use images::*;
pub use models::*;
pub use realtime::*;
pub use rerank::*;
pub use video::*;
