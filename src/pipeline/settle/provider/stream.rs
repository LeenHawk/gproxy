//! Streaming settlement guard for provider-shaped image operations.

use bytes::Bytes;

use crate::app::AppState;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::protocol::Provider as Family;
use crate::usage::Ended;

use super::Captured;
use super::image_sse::ImageSseCapture;

/// Exactly-once image-stream settlement. EOF after a protocol completion event
/// records `Complete`; an early EOF, transport error, or downstream cancellation
/// records `Interrupted` and still reconciles any pending quota charge.
pub(crate) struct StreamGuard {
    inner: Option<(Captured, ImageSseCapture)>,
}

impl StreamGuard {
    pub(crate) fn new(
        state: &AppState,
        ctx: &RequestCtx,
        cand: &Candidate,
        family: Family,
        actual_service_tier: Option<&str>,
    ) -> Self {
        Self {
            inner: Some((
                Captured::new(state, ctx, cand, family, actual_service_tier),
                ImageSseCapture::new(),
            )),
        }
    }

    pub(crate) fn push(&mut self, chunk: &Bytes) {
        if let Some((_, capture)) = self.inner.as_mut() {
            capture.push(chunk);
        }
    }

    pub(crate) fn request_id(&self) -> &str {
        &self
            .inner
            .as_ref()
            .expect("provider stream settle guard is active")
            .0
            .ctx
            .request_id
    }

    pub(crate) async fn finish_inline(mut self) -> super::super::Settlement {
        let ended = self.eof_ended();
        let parts = self
            .inner
            .take()
            .expect("provider stream settle guard is active");
        settle_stream(parts, ended).await
    }

    fn eof_ended(&self) -> Ended {
        if self
            .inner
            .as_ref()
            .is_some_and(|(_, capture)| capture.completed())
        {
            Ended::Complete
        } else {
            Ended::Interrupted
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn complete(&mut self, ended: Ended) {
        let Some(parts) = self.inner.take() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(settle_stream(parts, ended));
            }
            Err(_) => {
                tracing::warn!(
                    request_id = %parts.0.ctx.request_id,
                    "no runtime at provider stream settle; usage dropped"
                );
            }
        }
    }
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.complete(Ended::Interrupted);
        #[cfg(target_arch = "wasm32")]
        if let Some(parts) = self.inner.take() {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = settle_stream(parts, Ended::Interrupted).await;
            });
        }
    }
}

type StreamParts = (Captured, ImageSseCapture);

async fn settle_stream((captured, capture): StreamParts, ended: Ended) -> super::super::Settlement {
    let body = capture.settlement_body();
    let settlement = super::settle_ended(&captured, &body, ended).await;
    crate::pipeline::capture::record_downstream_response(
        &captured.state,
        &captured.ctx.request_id,
        &capture.log_body(),
    )
    .await;
    settlement
}
