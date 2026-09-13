//! Direct Gemini GenerateContent ↔ OpenAI Responses buffered conversion.
mod config;
mod content;
mod history;
mod identity;
mod logs;
mod media;
mod request;
mod response;
mod tools;
mod usage;
pub use identity::{GeminiReplayContext, RestoredGeminiPart};
pub use request::{gemini_to_responses_request, responses_to_gemini_request};
pub use response::{
    GeminiResponseContext, GeminiUsageFacts, gemini_to_responses_response,
    responses_to_gemini_response,
};
