//! Typed OpenAI/Gemini embedding conversions.
//!
//! Base64 vectors use the OpenAI embedding wire convention: consecutive
//! IEEE-754 binary32 values encoded in little-endian byte order. JSON base64
//! strings are never treated as vector text.

mod common;
mod request;
mod response;

pub use request::{
    PreparedEmbedding, gemini_batch_to_openai, gemini_single_to_openai, openai_to_gemini_batch,
    openai_to_gemini_single,
};
pub use response::{
    EmbeddingResponseContext, OpenAiResponseSupplement, OpenAiUsageFacts,
    gemini_batch_response_to_openai, gemini_single_response_to_openai,
    openai_batch_response_to_gemini, openai_single_response_to_gemini,
};
