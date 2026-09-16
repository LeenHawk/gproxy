use std::{fmt, pin::Pin};

use futures_core::Stream;

use super::{Bytes, HeaderMap, TransportError};

/// Raw HTTP body chunks. A transport chunk need not align with a JSON value,
/// UTF-8 character, SSE event, or multipart boundary.
#[cfg(not(target_arch = "wasm32"))]
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + Send + 'static>>;

#[cfg(target_arch = "wasm32")]
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + 'static>>;

/// An encoded HTTP body. Content type and encoding belong to the HTTP headers.
/// Both JSON and multipart can arrive buffered or streamed.
pub enum HttpBody {
    Bytes(Bytes),
    Stream(ByteStream),
}

impl fmt::Debug for HttpBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes(bytes) => f.debug_struct("Bytes").field("len", &bytes.len()).finish(),
            Self::Stream(_) => f.write_str("Stream(..)"),
        }
    }
}

/// How records are delimited inside an HTTP stream, independently of the
/// transport's chunk boundaries and the vendor's JSON schema.
///
/// This is format metadata, not a decoder. SSE fields remain part of the SSE
/// encoding until a decoder handles them; a byte chunk is never assumed to be
/// a complete event or JSON value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFraming {
    Sse,
    /// A single JSON array delivered incrementally, including `[` and `]`.
    JsonArray,
    /// Separate JSON values delimited by newlines (NDJSON).
    NdJson,
}

/// Parsed multipart parts, delivered in their original order. Repeated field
/// names are retained rather than collapsed into a map.
#[cfg(not(target_arch = "wasm32"))]
pub type PartStream =
    Pin<Box<dyn Stream<Item = Result<MultipartPart, TransportError>> + Send + 'static>>;

#[cfg(target_arch = "wasm32")]
pub type PartStream = Pin<Box<dyn Stream<Item = Result<MultipartPart, TransportError>> + 'static>>;

/// A decoded multipart body. Each part's headers retain its content disposition
/// (including field name and filename), content type, and extension headers.
///
/// MIME boundary handling belongs to the codec and the outer Content-Type
/// header. This view models parts, not boundary bytes. The parser adapter owns
/// coordination between the part stream and each part's content stream.
pub struct Multipart {
    pub parts: PartStream,
}

impl fmt::Debug for Multipart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Multipart { parts: .. }")
    }
}

#[derive(Debug)]
pub struct MultipartPart {
    pub headers: HeaderMap,
    pub body: HttpBody,
}
