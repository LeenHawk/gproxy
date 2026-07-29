//! Capture decorator for channel-owned custom exchanges.

use std::sync::Arc;

use bytes::Bytes;
#[cfg(not(target_arch = "wasm32"))]
use http::{HeaderMap, StatusCode};

use super::redaction::body_string;
use super::{UpstreamWire, insert_upstream_raw};
use crate::app::AppState;
use crate::http::client::{ClientError, UpstreamClient};
use crate::util::time::unix_now_ms;

#[cfg(not(target_arch = "wasm32"))]
struct CapturingSocket {
    inner: Box<dyn crate::http::client::ConduitSocket>,
    guard: crate::pipeline::stream::RawCaptureGuard,
    redact: bool,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl crate::http::client::ConduitSocket for CapturingSocket {
    async fn send_text(&mut self, text: String) -> Result<(), ClientError> {
        self.inner.send_text(text).await
    }

    async fn recv_text(&mut self) -> Option<Result<String, ClientError>> {
        let item = self.inner.recv_text().await;
        if let Some(Ok(text)) = item.as_ref() {
            let mut frame = body_string(text.as_bytes(), self.redact);
            frame.push('\n');
            self.guard.push(&Bytes::from(frame));
        }
        item
    }
}

/// A transparent [`UpstreamClient`] decorator that logs every transport call a
/// channel-owned custom exchange makes. Inserts are awaited to preserve
/// sequential call order; a streaming call then owns its row id for race-free
/// body backfill.
pub struct CapturingClient {
    inner: Arc<dyn UpstreamClient>,
    state: AppState,
    request_id: String,
    provider_id: i64,
    credential_id: i64,
}

impl CapturingClient {
    pub fn new(
        inner: Arc<dyn UpstreamClient>,
        state: AppState,
        request_id: String,
        provider_id: i64,
        credential_id: i64,
    ) -> Self {
        Self {
            inner,
            state,
            request_id,
            provider_id,
            credential_id,
        }
    }

    fn enabled(&self) -> bool {
        self.state.cp().log_settings.enable_upstream_log
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn capture_body(&self) -> bool {
        self.state.cp().log_settings.enable_upstream_log_body
    }

    async fn insert(&self, wire: UpstreamWire<'_>) -> Option<super::UpstreamCaptureId> {
        insert_upstream_raw(
            &self.state,
            &self.request_id,
            self.provider_id,
            self.credential_id,
            wire,
        )
        .await
    }
}

fn websocket_response_for_log(body: &Bytes, redact: bool) -> Bytes {
    let mut decoder = crate::transform::common::sse::SseDecoder::new();
    let mut frames = decoder.push(body);
    if let Some(frame) = decoder.finish() {
        frames.push(frame);
    }
    if frames.is_empty() {
        return body.clone();
    }

    let mut output = Vec::new();
    for frame in frames {
        output.extend_from_slice(body_string(frame.data.as_bytes(), redact).as_bytes());
        output.push(b'\n');
    }
    Bytes::from(output)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl UpstreamClient for CapturingClient {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        if !self.enabled() {
            return self.inner.send(req).await;
        }
        let url = req.uri().to_string();
        let method = req.method().clone();
        let sent_headers = req.headers().clone();
        let sent_body = req.body().clone();
        let start_ms = unix_now_ms();
        let resp = self.inner.send(req).await?;
        let latency_ms = unix_now_ms().saturating_sub(start_ms) as i64;
        let resp_body = resp.body().clone();
        self.insert(UpstreamWire {
            status: resp.status(),
            latency_ms,
            url: &url,
            method: &method,
            sent_headers: Some(&sent_headers),
            sent_body: &sent_body,
            resp_body: Some(&resp_body),
        })
        .await;
        Ok(resp)
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<(StatusCode, HeaderMap, crate::http::client::RespStream), ClientError> {
        // Preserve true streaming when capture is disabled. The trait default
        // buffers through `send`, which deadlocks exchanges kept open for a
        // later tool-result request.
        if !self.enabled() {
            return self.inner.send_streaming(req).await;
        }
        let url = req.uri().to_string();
        let method = req.method().clone();
        let sent_headers = req.headers().clone();
        let sent_body = req.body().clone();
        let start_ms = unix_now_ms();
        let (status, headers, stream) = self.inner.send_streaming(req).await?;
        let latency_ms = unix_now_ms().saturating_sub(start_ms) as i64;
        let capture_id = self
            .insert(UpstreamWire {
                status,
                latency_ms,
                url: &url,
                method: &method,
                sent_headers: Some(&sent_headers),
                sent_body: &sent_body,
                resp_body: None,
            })
            .await;
        let stream = match (self.capture_body(), capture_id) {
            (true, Some(capture_id)) => crate::pipeline::stream::capture_raw_stream(
                stream,
                crate::pipeline::stream::RawCaptureGuard::new(self.state.clone(), capture_id),
            ),
            _ => stream,
        };
        Ok((status, headers, stream))
    }

    async fn send_websocket(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<http::Response<Bytes>, ClientError> {
        if !self.enabled() {
            return self.inner.send_websocket(req).await;
        }
        let url = req.uri().to_string();
        let method = req.method().clone();
        let sent_headers = req.headers().clone();
        let sent_body = req.body().clone();
        let start_ms = unix_now_ms();
        let resp = self.inner.send_websocket(req).await?;
        let latency_ms = unix_now_ms().saturating_sub(start_ms) as i64;
        let resp_body = websocket_response_for_log(
            resp.body(),
            !self.state.cp().log_settings.disable_log_redaction,
        );
        self.insert(UpstreamWire {
            status: resp.status(),
            latency_ms,
            url: &url,
            method: &method,
            sent_headers: Some(&sent_headers),
            sent_body: &sent_body,
            resp_body: Some(&resp_body),
        })
        .await;
        Ok(resp)
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn open_websocket(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<Box<dyn crate::http::client::ConduitSocket>, ClientError> {
        if !self.enabled() {
            return self.inner.open_websocket(req).await;
        }
        let url = req.uri().to_string();
        let method = req.method().clone();
        let sent_headers = req.headers().clone();
        let sent_body = req.body().clone();
        let start_ms = unix_now_ms();
        let socket = self.inner.open_websocket(req).await?;
        let latency_ms = unix_now_ms().saturating_sub(start_ms) as i64;
        // `open_websocket` only exposes the successful upgrade metadata here.
        // The returned decorator backfills received frames into this same row.
        let capture_id = self
            .insert(UpstreamWire {
                status: StatusCode::SWITCHING_PROTOCOLS,
                latency_ms,
                url: &url,
                method: &method,
                sent_headers: Some(&sent_headers),
                sent_body: &sent_body,
                resp_body: None,
            })
            .await;
        match (self.capture_body(), capture_id) {
            (true, Some(capture_id)) => Ok(Box::new(CapturingSocket {
                inner: socket,
                guard: crate::pipeline::stream::RawCaptureGuard::new(
                    self.state.clone(),
                    capture_id,
                ),
                // Do not call the warning helper per frame; final backfill emits
                // the record-level warning when redaction is disabled.
                redact: !self.state.cp().log_settings.disable_log_redaction,
            })),
            _ => Ok(socket),
        }
    }
}

#[cfg(all(
    test,
    not(target_arch = "wasm32"),
    feature = "persist-db",
    feature = "cache-memory"
))]
#[path = "client_tests.rs"]
mod tests;
