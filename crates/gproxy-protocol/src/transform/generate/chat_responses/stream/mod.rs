//! Direct, typed incremental OpenAI Chat ↔ Responses conversion.
//!
//! These adapters deliberately keep the source collector and target framing
//! separate.  A source event is validated first, then the corresponding
//! target event is emitted; no intermediate JSON or completed source object is
//! made for partial events.

mod chat_to_responses;
mod common;
mod responses_to_chat;

pub use chat_to_responses::{ChatToResponsesContext, ChatToResponsesStream};
pub use common::{StreamEnd, StreamLimits};
pub use responses_to_chat::ResponsesToChatStream;
