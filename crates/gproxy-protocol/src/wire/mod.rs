//! Vendor wire dialect models.

pub mod claude;
mod declared;
pub use declared::DeclaredFields;
pub mod gemini;
pub mod openai;
