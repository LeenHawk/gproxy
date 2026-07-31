//! Native (wreq) implementation of [`UpstreamClient`].
//!
//! Translates `http::Request<Bytes>` -> wreq request -> `http::Response<Bytes>`.

use bytes::Bytes;

use crate::config::{UPSTREAM_CONNECT_TIMEOUT, UPSTREAM_READ_TIMEOUT, UPSTREAM_TOTAL_TIMEOUT};

use super::proxy_url::normalize_proxy_url;
use super::{ClientError, ConduitFrame, ConduitSocket, RespStream, UpstreamClient};

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
    fn prepare_request(&self, mut req: http::Request<Bytes>) -> Result<wreq::Request, ClientError> {
        let options = req.extensions_mut().remove::<super::TransportOptions>();
        let mut request: wreq::Request = req.into();
        let Some(options) = options else {
            return Ok(request);
        };
        if options.omit_body {
            *request.body_mut() = None;
        }

        let mut builder = wreq::RequestBuilder::from_parts(self.inner.clone(), request);
        if let Some(timeout) = options.total_timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(max_redirects) = options.max_redirects {
            let policy = if max_redirects == 0 {
                wreq::redirect::Policy::none()
            } else {
                wreq::redirect::Policy::limited(max_redirects)
            };
            builder = builder.redirect(policy);
        }
        if let Some(version) = options.http_version {
            builder = builder.version(version);
        }
        builder
            .build()
            .map_err(|error| ClientError::Config(error.to_string()))
    }

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
            builder = builder.proxy(wreq::Proxy::all(&*normalize_proxy_url(proxy_url))?);
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
    /// hit Cloudflare-fronted `claude.ai` (which rejects non-browser TLS) and
    /// MUST ride the same egress proxy as the credential's traffic
    /// (providers risk-score by source IP). Best-effort: verify against live
    /// origins, as Cloudflare's checks evolve.
    pub fn browser_with_proxy(proxy_url: Option<&str>) -> wreq::Result<Self> {
        let mut builder =
            with_timeouts(wreq::Client::builder()).emulation(wreq_util::Emulation::Chrome148);
        if let Some(proxy_url) = proxy_url {
            builder = builder.proxy(wreq::Proxy::all(&*normalize_proxy_url(proxy_url))?);
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
        let wreq_req = self.prepare_request(req)?;

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

        let wreq_req = self.prepare_request(req)?;
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

    async fn open_websocket(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<Box<dyn ConduitSocket>, ClientError> {
        let (mut parts, _) = req.into_parts();
        let options = parts.extensions.remove::<super::TransportOptions>();
        let deadline = options
            .and_then(|options| options.total_timeout)
            .map(websocket_deadline)
            .transpose()?;

        // Build from RequestBuilder so redirect and request timeout policy reach
        // the handshake. WebSocketRequestBuilder itself only exposes version.
        let mut request = self
            .inner
            .request(http::Method::GET, parts.uri.to_string())
            .headers(parts.headers);
        if let Some(options) = options {
            if let Some(timeout) = options.total_timeout {
                request = request.timeout(timeout);
            }
            if let Some(max_redirects) = options.max_redirects {
                request = request.redirect(redirect_policy(max_redirects));
            }
        }
        let mut request = wreq::ws::WebSocketRequestBuilder::new(request);
        if let Some(version) = options.and_then(|options| options.http_version) {
            request = request.version(version);
        }

        let open = async {
            let resp = request
                .send()
                .await
                .map_err(|e| ClientError::Transport(format!("websocket handshake: {e}")))?;
            resp.into_websocket()
                .await
                .map_err(|e| ClientError::Transport(format!("websocket upgrade: {e}")))
        };
        let ws = match deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, open)
                .await
                .map_err(|_| websocket_timeout())??,
            None => open.await?,
        };
        Ok(Box::new(WreqConduit { ws, deadline }))
    }
}

fn redirect_policy(max_redirects: usize) -> wreq::redirect::Policy {
    if max_redirects == 0 {
        wreq::redirect::Policy::none()
    } else {
        wreq::redirect::Policy::limited(max_redirects)
    }
}

fn websocket_deadline(timeout: std::time::Duration) -> Result<tokio::time::Instant, ClientError> {
    std::time::Instant::now()
        .checked_add(timeout)
        .map(tokio::time::Instant::from_std)
        .ok_or_else(|| ClientError::Config("websocket total_timeout is too large".into()))
}

fn websocket_timeout() -> ClientError {
    ClientError::Transport("websocket exceeded total_timeout".into())
}

/// [`ConduitSocket`] over a wreq [`wreq::ws::WebSocket`]. The frame API preserves
/// text, binary, and close frames; the compatibility text API skips binary.
struct WreqConduit {
    ws: wreq::ws::WebSocket,
    deadline: Option<tokio::time::Instant>,
}

#[async_trait::async_trait]
impl ConduitSocket for WreqConduit {
    async fn send_text(&mut self, text: String) -> Result<(), ClientError> {
        self.send_message(wreq::ws::message::Message::text(text))
            .await
    }

    async fn send_binary(&mut self, bytes: Bytes) -> Result<(), ClientError> {
        self.send_message(wreq::ws::message::Message::binary(bytes))
            .await
    }

    async fn recv_text(&mut self) -> Option<Result<String, ClientError>> {
        loop {
            match self.recv_frame().await? {
                Ok(ConduitFrame::Text(text)) => return Some(Ok(text)),
                Ok(ConduitFrame::Binary(_)) => continue,
                Ok(ConduitFrame::Close) => return None,
                Err(error) => return Some(Err(error)),
            }
        }
    }

    async fn recv_frame(&mut self) -> Option<Result<ConduitFrame, ClientError>> {
        loop {
            let message = match self.recv_message().await? {
                Ok(message) => message,
                Err(error) => return Some(Err(error)),
            };
            use wreq::ws::message::Message;
            match message {
                Message::Text(text) => {
                    return Some(Ok(ConduitFrame::Text(text.as_str().to_owned())));
                }
                Message::Binary(bytes) => return Some(Ok(ConduitFrame::Binary(bytes))),
                Message::Close(_) => return Some(Ok(ConduitFrame::Close)),
                Message::Ping(_) | Message::Pong(_) => continue,
            }
        }
    }

    async fn close(&mut self) -> Result<(), ClientError> {
        self.send_message(wreq::ws::message::Message::close(None))
            .await
    }
}

impl WreqConduit {
    async fn send_message(
        &mut self,
        message: wreq::ws::message::Message,
    ) -> Result<(), ClientError> {
        let send = self.ws.send(message);
        let result = match self.deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, send)
                .await
                .map_err(|_| websocket_timeout())?,
            None => send.await,
        };
        result.map_err(|e| ClientError::Transport(format!("websocket send: {e}")))
    }

    async fn recv_message(&mut self) -> Option<Result<wreq::ws::message::Message, ClientError>> {
        let recv = self.ws.recv();
        let received = match self.deadline {
            Some(deadline) => match tokio::time::timeout_at(deadline, recv).await {
                Ok(received) => received,
                Err(_) => return Some(Err(websocket_timeout())),
            },
            None => recv.await,
        };
        match received {
            Some(Ok(message)) => Some(Ok(message)),
            Some(Err(error)) => Some(Err(ClientError::Transport(format!(
                "websocket recv: {error}"
            )))),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_options_can_omit_an_empty_get_body() {
        let client = WreqClient::new();
        let mut request = http::Request::get("https://example.com/api/oauth/usage")
            .body(Bytes::new())
            .unwrap();
        request
            .extensions_mut()
            .insert(super::super::TransportOptions {
                http_version: Some(http::Version::HTTP_11),
                omit_body: true,
                ..Default::default()
            });

        let request = client.prepare_request(request).unwrap();
        assert!(request.body().is_none());
        assert_eq!(request.version(), Some(http::Version::HTTP_11));
    }
}
