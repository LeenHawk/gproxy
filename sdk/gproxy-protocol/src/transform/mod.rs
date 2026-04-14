//! Cross-protocol transforms for LLM API requests and responses.
//!
//! Transforms are organized in a three-dimensional matrix:
//! **source protocol** x **operation** x **target protocol**.
//!
//! ```text
//! transform/
//! ├── claude/                          # Source: Claude Messages API
//! │   ├── generate_content/
//! │   │   ├── gemini/                  # → Gemini GenerateContent
//! │   │   ├── openai_chat_completions/ # → OpenAI ChatCompletions
//! │   │   └── openai_response/         # → OpenAI Responses
//! │   ├── stream_generate_content/     # Same targets, streaming variant
//! │   ├── count_tokens/                # Token counting
//! │   ├── model_list/ & model_get/     # Model catalog
//! │   ├── nonstream_to_stream          # Full response → stream events
//! │   └── stream_to_nonstream          # Stream events → full response
//! ├── openai/                          # Source: OpenAI APIs
//! │   ├── generate_content/
//! │   │   ├── openai_chat_completions/ # ChatCompletions → other targets
//! │   │   └── openai_response/         # Responses → other targets
//! │   ├── stream_generate_content/     # Streaming variants
//! │   ├── compact/                     # Responses API compaction
//! │   ├── create_image/ & create_video/# Media generation
//! │   └── websocket/                   # WebSocket ↔ HTTP bridge
//! └── gemini/                          # Source: Gemini API
//!     ├── generate_content/            # → Claude / OpenAI targets
//!     ├── stream_generate_content/     # Streaming variants
//!     └── websocket/                   # Live API WebSocket ↔ HTTP bridge
//! ```
//!
//! # Non-streaming transforms
//!
//! Non-streaming transforms use `TryFrom` for type-safe conversion:
//!
//! ```rust,ignore
//! use gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest;
//! use gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest;
//!
//! let openai_req = OpenAiChatCompletionsRequest::try_from(claude_req)?;
//! ```
//!
//! # Streaming transforms
//!
//! Stream converters are stateful. They process one chunk at a time and push
//! output events into a caller-provided buffer (zero allocation after warmup):
//!
//! ```rust,ignore
//! let mut converter = OpenAiChatCompletionsToClaudeStream::default();
//! let mut buf = Vec::new();
//!
//! // Per chunk:
//! converter.on_chunk(chunk, &mut buf);
//! for event in buf.drain(..) { /* forward downstream */ }
//!
//! // On stream end:
//! converter.finish(&mut buf);
//! for event in buf.drain(..) { /* forward remaining */ }
//! ```
//!
//! # Format conversion utilities
//!
//! Each source protocol provides `nonstream_to_stream` and `stream_to_nonstream`
//! converters for bridging between streaming and non-streaming representations:
//!
//! ```rust,ignore
//! use gproxy_protocol::transform::claude::nonstream_to_stream::nonstream_to_stream;
//! use gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent;
//!
//! let mut events = Vec::new();
//! nonstream_to_stream(full_response, &mut events)?;
//! ```
//!
//! # WebSocket bridge
//!
//! OpenAI and Gemini provide `websocket/from_http` and `websocket/to_http` submodules
//! that bridge between HTTP request/response types and WebSocket frame types.
//! A `context` struct collects non-fatal warnings about dropped fields or
//! downgraded features during conversion.

pub mod claude;
pub mod dispatch;
pub mod gemini;
pub mod openai;
pub mod utils;

pub use utils::{TransformError, TransformResult};
