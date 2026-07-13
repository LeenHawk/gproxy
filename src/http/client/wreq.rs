//! Native (wreq) implementation of [`UpstreamClient`].
//!
//! Translates `http::Request<Bytes>` -> wreq request -> `http::Response<Bytes>`.

use bytes::Bytes;

use crate::config::{UPSTREAM_CONNECT_TIMEOUT, UPSTREAM_READ_TIMEOUT, UPSTREAM_TOTAL_TIMEOUT};

use super::{ClientError, ConduitSocket, RespStream, UpstreamClient};

/// Default upstream User-Agent for requests that don't set one and aren't
/// emulating a captured client. API-key channels (openai/deepseek/…) forward no
/// UA, so without this they'd send wreq's library default; this identifies the
/// proxy honestly instead. A configured `tls_fingerprint` (its
/// `headers.user-agent`) or a channel that injects its own UA overrides it.
const DEFAULT_USER_AGENT: &str = concat!("gproxy/", env!("CARGO_PKG_VERSION"));

/// Apply the shared transport bounds: connect cap + per-read idle cap (the
/// latter kills silent stalls without capping an active stream's total
/// duration). Total non-stream duration is bounded in [`WreqClient::send`].
fn with_timeouts(builder: wreq::ClientBuilder) -> wreq::ClientBuilder {
    builder
        .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
        .read_timeout(UPSTREAM_READ_TIMEOUT)
}

/// Upstream client backed by [`wreq::Client`] (native, TLS-emulation capable).
#[derive(Clone)]
pub struct WreqClient {
    inner: wreq::Client,
}

impl WreqClient {
    /// Build a `WreqClient` with a default [`wreq::Client`].
    ///
    /// Auto-decompression (`gzip`/`brotli`/`zstd`/`deflate`) is enabled: the
    /// client transparently inflates compressed responses, so downstream
    /// transform/billing logic always sees the plaintext body.
    pub fn new() -> Self {
        Self {
            inner: with_timeouts(wreq::Client::builder())
                .user_agent(DEFAULT_USER_AGENT)
                .build()
                .expect("default wreq client builds"),
        }
    }

    /// Build a `WreqClient` with an optional all-traffic upstream proxy.
    pub fn with_proxy_url(proxy_url: Option<&str>) -> wreq::Result<Self> {
        Self::with_proxy_and_emulation(proxy_url, None)
    }

    /// Build a `WreqClient` with an optional proxy and an optional TLS/header
    /// [`wreq::Emulation`] (§7.4). The pool builds one of these per distinct
    /// `(proxy, fingerprint)` target.
    pub fn with_proxy_and_emulation(
        proxy_url: Option<&str>,
        emulation: Option<wreq::Emulation>,
    ) -> wreq::Result<Self> {
        let mut builder = with_timeouts(wreq::Client::builder());
        if let Some(proxy_url) = proxy_url {
            builder = builder.proxy(wreq::Proxy::all(proxy_url)?);
        }
        // A fingerprint carries its own UA via emulation headers; only fall back
        // to the proxy's default UA when no emulation is applied, so the default
        // never shadows a configured fingerprint's user-agent.
        match emulation {
            Some(emulation) => builder = builder.emulation(emulation),
            None => builder = builder.user_agent(DEFAULT_USER_AGENT),
        }
        Ok(Self {
            inner: builder.build()?,
        })
    }

    /// Build a `WreqClient` impersonating a real Chrome browser (TLS + HTTP/2 +
    /// headers, via `wreq-util`'s captured emulation), optionally routed through
    /// `proxy_url`. Used for the cookie → OAuth exchange + cookie refresh, which
    /// hit Cloudflare-fronted `claude.ai` / `chatgpt.com` (reject non-browser
    /// TLS) and MUST ride the same egress proxy as the credential's traffic
    /// (providers risk-score by source IP). Best-effort: verify against live
    /// origins, as Cloudflare's checks evolve.
    pub fn browser_with_proxy(proxy_url: Option<&str>) -> wreq::Result<Self> {
        let mut builder =
            with_timeouts(wreq::Client::builder()).emulation(wreq_util::Emulation::Chrome148);
        if let Some(proxy_url) = proxy_url {
            builder = builder.proxy(wreq::Proxy::all(proxy_url)?);
        }
        Ok(Self {
            inner: builder.build()?,
        })
    }

    /// [`browser_with_proxy`](Self::browser_with_proxy) with no proxy.
    pub fn browser() -> wreq::Result<Self> {
        Self::browser_with_proxy(None)
    }
}

impl Default for WreqClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl UpstreamClient for WreqClient {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        // http::Request<Bytes> -> wreq::Request via From impl (Bytes: Into<wreq::Body>).
        let wreq_req: wreq::Request = req.into();

        // Non-stream: the whole exchange (connect → full body) is bounded, on
        // top of the builder's connect/read caps — a trickling upstream can't
        // hold a gateway slot indefinitely.
        let fut = async {
            let resp = self
                .inner
                .execute(wreq_req)
                .await
                .map_err(|e| ClientError::Transport(e.to_string()))?;

            let status = resp.status();
            let headers = resp.headers().clone();
            let body_bytes = resp
                .bytes()
                .await
                .map_err(|e| ClientError::Transport(e.to_string()))?;
            Ok::<_, ClientError>((status, headers, body_bytes))
        };
        let (status, headers, body_bytes) = tokio::time::timeout(UPSTREAM_TOTAL_TIMEOUT, fut)
            .await
            .map_err(|_| {
                ClientError::Transport(format!(
                    "upstream exceeded {}s total timeout",
                    UPSTREAM_TOTAL_TIMEOUT.as_secs()
                ))
            })??;

        let mut builder = http::Response::builder().status(status);
        if let Some(hmap) = builder.headers_mut() {
            *hmap = headers;
        }
        builder
            .body(body_bytes)
            .map_err(|e| ClientError::Transport(e.to_string()))
    }

    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<(http::StatusCode, http::HeaderMap, RespStream), ClientError> {
        use futures_util::{StreamExt, TryStreamExt};

        let wreq_req: wreq::Request = req.into();
        let resp = self
            .inner
            .execute(wreq_req)
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        // bytes_stream(self) consumes the owned Response (→ 'static) and yields
        // wreq::Result<Bytes>; map the error to ClientError so the item type
        // matches RespStream.
        let stream: RespStream = resp
            .bytes_stream()
            .map_err(|e| ClientError::Transport(e.to_string()))
            .boxed();
        Ok((status, headers, stream))
    }

    /// Open a conduit WebSocket via wreq's `ws` support — rides this client's
    /// proxy + TLS emulation (Cloudflare-fronted `ws.chatgpt.com` rejects
    /// non-browser TLS). The url carries its own `?verify=` auth.
    async fn open_conduit(&self, url: &str) -> Result<Box<dyn ConduitSocket>, ClientError> {
        let resp = self
            .inner
            .websocket(url)
            .send()
            .await
            .map_err(|e| ClientError::Transport(format!("conduit handshake: {e}")))?;
        let ws = resp
            .into_websocket()
            .await
            .map_err(|e| ClientError::Transport(format!("conduit upgrade: {e}")))?;
        Ok(Box::new(WreqConduit { ws }))
    }

    async fn open_websocket(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<Box<dyn ConduitSocket>, ClientError> {
        let uri = req.uri().to_string();
        let headers = req.headers().clone();
        let resp = self
            .inner
            .websocket(uri)
            .headers(headers)
            .send()
            .await
            .map_err(|e| ClientError::Transport(format!("websocket handshake: {e}")))?;
        let ws = resp
            .into_websocket()
            .await
            .map_err(|e| ClientError::Transport(format!("websocket upgrade: {e}")))?;
        Ok(Box::new(WreqConduit { ws }))
    }
}

/// [`ConduitSocket`] over a wreq [`wreq::ws::WebSocket`]. Receives skip non-text
/// frames (ping/pong/binary) so the caller only sees the JSON envelopes.
struct WreqConduit {
    ws: wreq::ws::WebSocket,
}

#[async_trait::async_trait]
impl ConduitSocket for WreqConduit {
    async fn send_text(&mut self, text: String) -> Result<(), ClientError> {
        self.ws
            .send(wreq::ws::message::Message::text(text))
            .await
            .map_err(|e| ClientError::Transport(format!("conduit send: {e}")))
    }

    async fn recv_text(&mut self) -> Option<Result<String, ClientError>> {
        loop {
            match self.ws.recv().await {
                Some(Ok(msg)) => match msg.into_text() {
                    Ok(t) => return Some(Ok(t.as_str().to_string())),
                    // Non-text frame (ping/pong/binary/close) — skip and keep reading.
                    Err(_) => continue,
                },
                Some(Err(e)) => {
                    return Some(Err(ClientError::Transport(format!("conduit recv: {e}"))));
                }
                None => return None,
            }
        }
    }
}
