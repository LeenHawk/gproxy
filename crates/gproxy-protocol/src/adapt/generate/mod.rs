//! Concrete generation edges over host-selected upstreams. Preparation owns the
//! original declared request and its return mapping; no routing or default client.

pub mod chat_claude;
pub mod chat_gemini;
pub mod chat_responses;
mod claude_chat_ids;
pub mod claude_gemini;
pub mod claude_responses;
pub mod fanout;
pub mod gemini_responses;
mod history;
mod identity_facts;
pub mod image_resources;
mod legacy_chat;
mod request_ids;
mod resources;
mod signed;
mod state;
pub mod stream;
mod transport;

pub use crate::transform::generate::chat_responses::ToolCallKind;
pub use resources::GenerationResources;
pub use state::{ChatCallForm, GenerationStateAccess, GenerationToolReplay};
pub use transport::{Endpoint, GenerationIdentity, GenerationOutcome, GenerationProgress};
