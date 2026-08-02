//! Raw upstream stream capture and response-body backfill.

use crate::app::AppState;
use crate::pipeline::capture::UpstreamCaptureId;
use crate::pipeline::outcome::ByteStream;
use crate::pipeline::settle::RelayBuffer;

/// Buffers bytes at the caller-selected seam and backfills one captured row.
pub struct RawCaptureGuard {
    inner: Option<(AppState, UpstreamCaptureId, RelayBuffer)>,
}

impl RawCaptureGuard {
    pub(crate) fn new(state: AppState, capture_id: UpstreamCaptureId) -> Self {
        Self {
            inner: Some((state, capture_id, RelayBuffer::new())),
        }
    }

    pub(crate) fn push(&mut self, chunk: &bytes::Bytes) {
        if let Some((_, _, buffer)) = self.inner.as_mut() {
            buffer.push(chunk.clone());
        }
    }

    fn flush(&mut self) {
        if let Some((state, capture_id, buffer)) = self.inner.take() {
            let bytes = buffer.concat_for_log();
            #[cfg(not(target_arch = "wasm32"))]
            tokio::spawn(async move {
                crate::pipeline::capture::record_upstream_response(&state, capture_id, &bytes)
                    .await;
            });
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(async move {
                crate::pipeline::capture::record_upstream_response(&state, capture_id, &bytes)
                    .await;
            });
        }
    }
}

impl Drop for RawCaptureGuard {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Tee upstream chunks into `guard` while passing them through unchanged.
pub fn capture_raw_stream(stream: ByteStream, guard: RawCaptureGuard) -> ByteStream {
    use futures_util::StreamExt;

    struct State {
        inner: Option<ByteStream>,
        guard: Option<RawCaptureGuard>,
    }

    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(stream),
            guard: Some(guard),
        },
        |mut state| async move {
            let inner = state.inner.as_mut()?;
            match inner.next().await {
                Some(Ok(chunk)) => {
                    if let Some(guard) = state.guard.as_mut() {
                        guard.push(&chunk);
                    }
                    Some((Ok(chunk), state))
                }
                Some(Err(error)) => {
                    state.inner = None;
                    drop(state.guard.take());
                    Some((Err(error), state))
                }
                None => {
                    state.inner = None;
                    drop(state.guard.take());
                    None
                }
            }
        },
    ))
}
