//! Gemini `models.streamGenerateContent` wire shapes.
//!
//! The endpoint takes the same request as `generateContent` and returns a
//! stream of `GenerateContentResponse` JSON records.  A transport chunk is
//! not a JSON record. SSE (`alt=sse`) or JSON-array framing belongs to the
//! transport; these aliases do not implement parsing or codecs.
//! The generated google-cloud-python REST method uses ResponseIterator with
//! `_rest_streaming_base`, whose JSON-array iterator expects an opening `[`.
//! Source: `upstream_docs/gemini/docs/Generating content.md`,
//! `models.streamGenerateContent` (endpoint, request, and response body).
use super::{GenerateContentRequest, GenerateContentResponseBody};
use crate::{WireResponse, connection::ByteStream};

/// Every streamed record has exactly the non-streaming response shape.
pub type StreamChunk = GenerateContentResponseBody;
pub type StreamRequest = GenerateContentRequest;
pub type StreamResponse = WireResponse<ByteStream>;
