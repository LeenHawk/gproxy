//! Concrete generation edges over host-selected upstreams. Preparation owns the
//! original declared request and its return mapping; no routing or default client.
pub mod chat_claude;
pub mod chat_gemini;
pub mod chat_responses;
pub mod claude_gemini;
pub mod claude_responses;
pub mod gemini_responses;
mod transport;
pub use transport::{Endpoint, GenerationIdentity, GenerationOutcome, GenerationProgress};

mod identity_facts;
mod state;
pub use state::{ChatCallForm, GenerationStateAccess, GenerationToolReplay};

mod history;

mod signed;

mod claude_chat_ids;

mod resources;
pub use resources::GenerationResources;

pub use crate::transform::generate::chat_responses::ToolCallKind;

mod request_ids;

pub mod stream;

pub mod fanout;

mod legacy_chat;

pub mod image_resources;
