//! Native (wreq) implementation of [`UpstreamClient`].
//!
//! Translates `http::Request<Bytes>` -> wreq request -> `http::Response<Bytes>`.

use bytes::Bytes;

use crate::config::{UPSTREAM_CONNECT_TIMEOUT, UPSTREAM_READ_TIMEOUT, UPSTREAM_TOTAL_TIMEOUT};

use super::{ClientError, RespStream, UpstreamClient};

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
    /// headers, via `wreq-util`'s captured emulation). Used for the claudecode
    /// cookie → OAuth exchange, which hits Cloudflare-fronted `claude.ai` and
    /// rejects non-browser TLS. Best-effort: verify against live claude.ai, as
    /// Cloudflare's checks evolve.
    pub fn browser() -> wreq::Result<Self> {
        Ok(Self {
            inner: with_timeouts(wreq::Client::builder())
                .emulation(wreq_util::Emulation::Chrome142)
                .build()?,
        })
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
}
