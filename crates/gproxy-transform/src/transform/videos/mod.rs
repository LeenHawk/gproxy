//! Video transforms: OpenAI video jobs <-> Gemini Veo long-running operations.
//!
//! Create maps the request bodies; retrieve reuses the same response mapping
//! (an OpenAI video object and a Veo operation are both "job status" shapes),
//! and its GET request has no body to convert. Content download is binary and
//! never passes through the transform layer.

mod common;
pub mod create;
