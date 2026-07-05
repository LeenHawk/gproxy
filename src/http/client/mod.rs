//! Outbound HTTP: a client-agnostic [`UpstreamClient`] trait with a native
//! (wreq) and an edge (fetch) implementation, selected by build target.

use bytes::Bytes;

/// Transport-level error from the upstream client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("upstream transport error: {0}")]
    Transport(String),
    /// Per-target client configuration is unusable (malformed proxy URL,
    /// fingerprint that maps to no emulation). Fails the attempt instead of
    /// silently downgrading to the default client (§7.4 policy bypass).
    #[error("upstream client config error: {0}")]
    Config(String),
}

/// Streaming response body (NATIVE only). Item error is [`ClientError`] — the
/// SAME typedef as [`crate::pipeline::outcome::ByteStream`], so the failover →
/// outcome → axum `Body::from_stream` handoff needs no re-box (`ClientError:
/// Error + Send + Sync + 'static` satisfies `Into<BoxError>`).
#[cfg(not(target_arch = "wasm32"))]
pub type RespStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, ClientError>> + Send>>;

/// An open conduit WebSocket (NATIVE only): text-frame send + receive. Kept
/// minimal and object-safe so [`UpstreamClient::open_conduit`] can hand one back
/// across the `dyn` boundary. Used by the chatgpt channel to consume the
/// `stream_handoff` conduit (`wss://ws.chatgpt.com/…`) for thinking-model turns.
#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
pub trait ConduitSocket: Send {
    /// Send one text frame.
    async fn send_text(&mut self, text: String) -> Result<(), ClientError>;
    /// Receive the next text frame; `None` when the socket closes. Non-text
    /// frames (ping/pong/binary) are skipped by the implementation.
    async fn recv_text(&mut self) -> Option<Result<String, ClientError>>;
}

/// Client-agnostic upstream HTTP transport. Native impl = wreq (supports TLS
/// emulation); edge impl = host `fetch`. The `?Send` on the wasm async_trait
/// controls the FUTURE; `Send + Sync` here constrains the implementing TYPE so
/// that `Arc<dyn UpstreamClient>` is usable in multi-threaded async contexts.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait UpstreamClient: Send + Sync {
    /// Send a fully-formed request and return the response (status + headers + body bytes).
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError>;

    /// Send a fully prepared Responses WebSocket request and return the
    /// collected response as an SSE body. Native normally uses
    /// [`open_websocket`](Self::open_websocket) for streaming; wasm edge uses
    /// this buffered hook because `ResponseBody::Stream` is native-only.
    async fn send_websocket(
        &self,
        _req: http::Request<Bytes>,
    ) -> Result<http::Response<Bytes>, ClientError> {
        Err(ClientError::Config(
            "upstream websocket not supported by this client".into(),
        ))
    }

    /// Streaming variant (NATIVE only): status + headers immediately, body as a
    /// `ClientError`-itemed byte stream. The default buffers via `send` and wraps
    /// the whole body as one chunk — a correct, lower-fidelity fallback;
    /// `WreqClient` overrides with `bytes_stream()`.
    #[cfg(not(target_arch = "wasm32"))]
    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<(http::StatusCode, http::HeaderMap, RespStream), ClientError> {
        use futures_util::StreamExt;
        let resp = self.send(req).await?;
        let (parts, body) = resp.into_parts();
        let once = futures_util::stream::once(async move { Ok::<Bytes, ClientError>(body) });
        Ok((parts.status, parts.headers, once.boxed()))
    }

    /// Open a conduit WebSocket to `url` (NATIVE only). The default returns a
    /// config error — only `WreqClient` supports it. The url already carries its
    /// own auth (`?verify=…`), so no extra headers are needed; the call rides
    /// this client's proxy + TLS emulation.
    #[cfg(not(target_arch = "wasm32"))]
    async fn open_conduit(&self, _url: &str) -> Result<Box<dyn ConduitSocket>, ClientError> {
        Err(ClientError::Config(
            "conduit websocket not supported by this client".into(),
        ))
    }

    /// Open a generic upstream WebSocket from a fully prepared request
    /// (NATIVE only). Headers are preserved for auth and beta flags; the body is
    /// owned by the caller and is not sent by the handshake.
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

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod pool;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub use pool::ClientPool;

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod wreq;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub use wreq::WreqClient;

#[cfg(all(target_arch = "wasm32", feature = "upstream-fetch"))]
mod fetch;
#[cfg(all(target_arch = "wasm32", feature = "upstream-fetch"))]
pub use fetch::FetchClient;
