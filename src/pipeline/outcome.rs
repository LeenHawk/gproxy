//! Unified executor output (§6.3): stream & non-stream share everything up to
//! classify and differ only in `body`.

use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::channel::disposition::Disposition;
use crate::http::client::ClientError;

/// Byte-stream of the upstream response body. Native carries `Send` for axum;
/// wasm stays local to its JS event-loop isolate.
///
/// **Item error is [`ClientError`]** end to end (one error type across
/// `send_streaming` → failover → `ExecOutcome` → axum `Body::from_stream`).
/// `ClientError: Error + Send + Sync + 'static`, so it satisfies
/// `Body::from_stream`'s `S::Error: Into<BoxError>` with no conversion. This is
/// the SAME typedef as [`crate::http::client::RespStream`] — assigned straight
/// across, no re-box.
#[cfg(not(target_arch = "wasm32"))]
pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, ClientError>> + Send>>;
#[cfg(target_arch = "wasm32")]
pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, ClientError>>>>;

/// Unified executor output (§6.3).
pub struct ExecOutcome {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ResponseBody,
    pub disposition: Disposition,
}

/// Response body — buffered, or a streaming SSE passthrough.
pub enum ResponseBody {
    Full(Bytes),
    /// Streaming passthrough. The stream is `Send` on native and JS-local on wasm.
    Stream(ByteStream),
}
