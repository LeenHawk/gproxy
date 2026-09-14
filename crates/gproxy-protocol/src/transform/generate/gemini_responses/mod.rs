//! Direct Gemini GenerateContent ↔ OpenAI Responses buffered conversion.
mod config;
pub(crate) mod content;
pub(crate) mod history;
pub(crate) mod identity;
mod logs;
mod media;
mod request;
mod response;
pub(crate) mod tools;
mod usage;
pub use identity::{GeminiReplayContext, RestoredGeminiPart};
pub use request::{gemini_to_responses_request, responses_to_gemini_request};
pub use response::{
    GeminiResponseContext, GeminiUsageFacts, gemini_to_responses_response,
    responses_to_gemini_response,
};

pub mod stream;
