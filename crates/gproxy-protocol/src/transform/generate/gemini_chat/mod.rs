//! Direct Gemini GenerateContent ↔ OpenAI Chat conversion.

mod config;
mod content;
mod logs;
mod media;
mod request;
mod response;
mod tools;
mod usage;

pub(crate) use request::gemini_to_openai_request_with_calls;
pub use request::{gemini_to_openai_request, openai_to_gemini_request};
pub use response::{
    GeminiChatResponseSupplement, gemini_to_openai_response, openai_to_gemini_response,
};

pub mod stream;
