//! Direct Gemini GenerateContent ↔ OpenAI Responses buffered conversion.

mod config;
pub(crate) mod content;
pub(crate) mod history;
pub(crate) mod identity;
mod images;
mod logs;
mod media;
mod request;
pub(crate) mod request_images;
mod response;
pub mod stream;
pub(crate) mod tools;
mod usage;

pub use identity::GeminiReplayContext;
pub use request::{gemini_to_responses_request, responses_to_gemini_request};
pub use response::{
    GeminiResponseContext, GeminiUsageFacts, gemini_to_responses_response,
    responses_to_gemini_response, responses_to_gemini_response_with_modalities,
};
