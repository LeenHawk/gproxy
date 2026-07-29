//! Host-provided upstream transport capability available to channel adapters.

use bytes::Bytes;

/// Per-request transport behavior carried through [`http::Request::extensions`].
///
/// All request entrypoints consume these options, including WebSocket ones. The
/// built-in native wreq transport enforces redirect/version policy on the
/// WebSocket handshake; a WebSocket `total_timeout` remains in force for the
/// socket's sends and receives. A native WebSocket handshake never sends the
/// request body, independently of `omit_body`.
///
/// The built-in Fetch transport enforces `total_timeout`, `omit_body`, and
/// `max_redirects = Some(0)`. For its terminal WebSocket round trip, the timeout
/// covers both Fetch and socket use, while `omit_body` suppresses the initial
/// application frame. Fetch cannot select an HTTP version or guarantee an exact
/// positive redirect bound, so it returns [`ClientError::Config`] when either
/// unsupported constraint is set; callers may leave those fields as `None` to
/// use the host Fetch policy.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransportOptions {
    pub total_timeout: Option<std::time::Duration>,
    pub max_redirects: Option<usize>,
    pub http_version: Option<http::Version>,
    pub omit_body: bool,
}

/// Synchronous decoder for a chunked upstream byte stream.
pub trait ByteStreamDecoder: Send {
    /// Feed one raw upstream chunk and return any decoded bytes.
    fn push(&mut self, chunk: &[u8]) -> Vec<u8>;

    /// Flush trailing buffered state at end of stream.
    fn finish(&mut self) -> Vec<u8>;
}

/// Transport-level error from the upstream client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("upstream transport error: {0}")]
    Transport(String),
    /// Per-target client configuration is unusable. The host fails the attempt
    /// instead of silently downgrading proxy or TLS policy.
    #[error("upstream client config error: {0}")]
    Config(String),
}

/// Streaming response body. Native streams are `Send`; wasm streams stay local
/// because Fetch `ReadableStream` handles are JS-bound.
#[cfg(not(target_arch = "wasm32"))]
pub type RespStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, ClientError>> + Send>>;
#[cfg(target_arch = "wasm32")]
pub type RespStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, ClientError>>>>;

/// An open upstream WebSocket (native only), kept minimal and object-safe.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
pub trait ConduitSocket: Send {
    async fn send_text(&mut self, text: String) -> Result<(), ClientError>;
    async fn recv_text(&mut self) -> Option<Result<String, ClientError>>;
}

/// Host-owned upstream capability. Implementations apply the resolved proxy,
/// TLS profile, capture policy, and platform transport; channels only construct
/// requests and interpret responses.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait UpstreamClient: Send + Sync {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError>;

    async fn send_websocket(
        &self,
        _req: http::Request<Bytes>,
    ) -> Result<http::Response<Bytes>, ClientError> {
        Err(ClientError::Config(
            "upstream websocket not supported by this client".into(),
        ))
    }

    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<(http::StatusCode, http::HeaderMap, RespStream), ClientError> {
        let resp = self.send(req).await?;
        let (parts, body) = resp.into_parts();
        let once = futures_util::stream::once(async move { Ok::<Bytes, ClientError>(body) });
        Ok((parts.status, parts.headers, Box::pin(once)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn open_websocket(
        &self,
        _req: http::Request<Bytes>,
    ) -> Result<Box<dyn ConduitSocket>, ClientError> {
        Err(ClientError::Config(
            "upstream websocket not supported by this client".into(),
        ))
    }
}
