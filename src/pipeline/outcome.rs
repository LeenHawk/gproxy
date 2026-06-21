//! Unified executor output (§6.3): stream & non-stream share everything up to
//! classify and differ only in `body`.

use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::channel::disposition::Disposition;
#[cfg(not(target_arch = "wasm32"))]
use crate::http::client::ClientError;

/// Byte-stream of the upstream response body.
///
/// **Item error is [`ClientError`]** end to end (one error type across
/// `send_streaming` → failover → `ExecOutcome` → axum `Body::from_stream`).
/// `ClientError: Error + Send + Sync + 'static`, so it satisfies
/// `Body::from_stream`'s `S::Error: Into<BoxError>` with no conversion. This is
/// the SAME typedef as [`crate::http::client::RespStream`] — assigned straight
/// across, no re-box. Native only (wasm reads whole bodies via `fetch`).
#[cfg(not(target_arch = "wasm32"))]
pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, ClientError>> + Send>>;

/// Unified executor output (§6.3).
pub struct ExecOutcome {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ResponseBody,
    pub disposition: Disposition,
}

/// Response body — buffered, or (native) a streaming SSE passthrough.
pub enum ResponseBody {
    Full(Bytes),
    /// Streaming passthrough (native only). On wasm this variant does not exist;
    /// the executor branch that builds it is `#[cfg(not(target_arch = "wasm32"))]`.
    #[cfg(not(target_arch = "wasm32"))]
    Stream(ByteStream),
}
