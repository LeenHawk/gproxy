//! Direct Claude Messages ↔ OpenAI Chat conversions.
//!
//! The pair operates on the declared wire DTOs. It never consults flattened
//! `rest` fields and never routes through Responses, Gemini, or a common content
//! representation.
//!
//! Existing tool-call IDs are preserved so call/result links remain stable.
//! Target-specific validation or rewriting must run in the host's IdentityFlow
//! before this pure pair; this module does not invent prefixes or signatures.
//! This module converts buffered messages only, not streaming event lifecycles.

mod content;
mod media;
mod request;
mod requirements;
mod response;
mod schema;
mod tools;
mod usage;
mod util;

pub use request::{claude_to_openai, openai_to_claude};
pub use response::{claude_response_to_openai, openai_response_to_claude};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResponseSupplement {
    pub created_unix_seconds: Option<i64>,
}

pub mod stream;
