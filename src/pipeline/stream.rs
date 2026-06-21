//! Streaming response tail (§6.4, D4). Native-only. Holds ONLY the body-side
//! conversion invoked by `failover` when materializing a streaming attempt — it
//! does not iterate candidates or call `classify`.

use crate::channel::ChannelStreamDecoder;
use crate::http::client::RespStream;
use crate::pipeline::outcome::ByteStream;
use crate::pipeline::settle::StreamGuard;
use crate::transform::stream_adapter::SseTransformer;

/// Convert a per-attempt streaming body source into the executor's `ByteStream`
/// unchanged (passthrough attempts). `RespStream` and `ByteStream` are the SAME
/// typedef (`Item = Result<Bytes, ClientError>`, D1), so this is the identity;
/// transform attempts splice [`transform_byte_stream`] instead.
pub fn into_byte_stream(s: RespStream) -> ByteStream {
    s
}

/// Wrap a streaming attempt with per-frame cross-protocol conversion. Frames
/// are re-chunked on SSE boundaries; upstream errors are forwarded once and
/// end the stream; the inbound terminator is emitted at upstream EOF.
pub fn transform_byte_stream(s: RespStream, t: SseTransformer) -> ByteStream {
    use bytes::Bytes;
    use futures_util::StreamExt;

    struct State {
        inner: Option<RespStream>,
        t: SseTransformer,
    }

    Box::pin(futures_util::stream::unfold(
        State { inner: Some(s), t },
        |mut st| async move {
            loop {
                let inner = st.inner.as_mut()?;
                match inner.next().await {
                    Some(Ok(chunk)) => {
                        let out = st.t.push(&chunk);
                        if out.is_empty() {
                            continue; // partial frame buffered; poll again
                        }
                        return Some((Ok(Bytes::from(out)), st));
                    }
                    Some(Err(e)) => {
                        st.inner = None;
                        return Some((Err(e), st));
                    }
                    None => {
                        st.inner = None;
                        let tail = st.t.finish();
                        if tail.is_empty() {
                            return None;
                        }
                        return Some((Ok(Bytes::from(tail)), st));
                    }
                }
            }
        },
    ))
}

/// Wrap a streaming attempt with a per-channel byte decoder, spliced BEFORE any
/// protocol transform (envelope unwrap / binary → SSE). Drives a
/// [`ChannelStreamDecoder`] exactly like [`transform_byte_stream`] drives an
/// `SseTransformer`: `push` per upstream chunk, `finish` at EOF; upstream errors
/// are forwarded once and end the stream. Its `ByteStream` output is then fed to
/// either the M2 transform ([`transform_byte_stream`]) or straight to the client
/// ([`into_byte_stream`]) by the caller.
pub fn channel_decode_stream(s: RespStream, decoder: Box<dyn ChannelStreamDecoder>) -> ByteStream {
    use bytes::Bytes;
    use futures_util::StreamExt;

    struct State {
        inner: Option<RespStream>,
        decoder: Box<dyn ChannelStreamDecoder>,
    }

    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(s),
            decoder,
        },
        |mut st| async move {
            loop {
                let inner = st.inner.as_mut()?;
                match inner.next().await {
                    Some(Ok(chunk)) => {
                        let out = st.decoder.push(&chunk);
                        if out.is_empty() {
                            continue; // partial frame buffered; poll again
                        }
                        return Some((Ok(Bytes::from(out)), st));
                    }
                    Some(Err(e)) => {
                        st.inner = None;
                        return Some((Err(e), st));
                    }
                    None => {
                        st.inner = None;
                        let tail = st.decoder.finish();
                        if tail.is_empty() {
                            return None;
                        }
                        return Some((Ok(Bytes::from(tail)), st));
                    }
                }
            }
        },
    ))
}

/// Wrap an already-materialized body stream with §17 settlement: every relayed
/// chunk is pushed (refcounted) into the guard's bounded buffer; upstream EOF
/// settles `Complete`; a mid-stream upstream error emits ONE protocol-shaped
/// error frame (不裸断) and the dropped guard settles `Interrupted`; a client
/// drop anywhere also settles `Interrupted` via the guard's Drop.
pub fn instrument_stream(s: ByteStream, guard: StreamGuard) -> ByteStream {
    use futures_util::StreamExt;

    struct State {
        inner: Option<ByteStream>,
        guard: Option<StreamGuard>,
    }

    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(s),
            guard: Some(guard),
        },
        |mut st| async move {
            let inner = st.inner.as_mut()?;
            match inner.next().await {
                Some(Ok(chunk)) => {
                    if let Some(g) = st.guard.as_mut() {
                        g.push(&chunk);
                    }
                    Some((Ok(chunk), st))
                }
                Some(Err(e)) => {
                    st.inner = None;
                    let kind = st.guard.as_ref().and_then(StreamGuard::inbound_kind);
                    tracing::warn!(error = %e, "upstream stream failed");
                    // dropping the guard settles Interrupted (after the frame)
                    drop(st.guard.take());
                    match kind {
                        Some(k) => Some((Ok(crate::pipeline::settle::frames::error_frame(k)), st)),
                        None => Some((Err(e), st)),
                    }
                }
                None => {
                    st.inner = None;
                    if let Some(g) = st.guard.take() {
                        g.finish();
                    }
                    None
                }
            }
        },
    ))
}

/// Buffers post-decode upstream response bytes for a streaming response and, on
/// EOF or client drop, backfills `upstream_requests.response_body` (§8-D, bounded
/// by `RelayBuffer`'s ~4MB cap). Native-only.
#[cfg(not(target_arch = "wasm32"))]
pub struct RawCaptureGuard {
    inner: Option<(
        crate::app::AppState,
        String,
        crate::pipeline::settle::RelayBuffer,
    )>,
}

#[cfg(not(target_arch = "wasm32"))]
impl RawCaptureGuard {
    pub fn new(state: crate::app::AppState, request_id: String) -> Self {
        Self {
            inner: Some((
                state,
                request_id,
                crate::pipeline::settle::RelayBuffer::new(),
            )),
        }
    }

    fn push(&mut self, chunk: &bytes::Bytes) {
        if let Some((_, _, buf)) = self.inner.as_mut() {
            buf.push(chunk.clone());
        }
    }

    /// Spawn the gated backfill of the buffered upstream response body.
    fn flush(&mut self) {
        if let Some((state, rid, buf)) = self.inner.take() {
            let bytes = buf.concat_for_log();
            tokio::spawn(async move {
                crate::pipeline::capture::record_upstream_response(&state, &rid, &bytes).await;
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for RawCaptureGuard {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Tee post-decode upstream chunks into `guard` while passing them through
/// unchanged. Spliced AFTER the channel decoder and BEFORE any protocol
/// transform, so it sees the provider's response in its native wire shape.
#[cfg(not(target_arch = "wasm32"))]
pub fn capture_raw_stream(s: ByteStream, guard: RawCaptureGuard) -> ByteStream {
    use futures_util::StreamExt;

    struct State {
        inner: Option<ByteStream>,
        guard: Option<RawCaptureGuard>,
    }

    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(s),
            guard: Some(guard),
        },
        |mut st| async move {
            let inner = st.inner.as_mut()?;
            match inner.next().await {
                Some(Ok(chunk)) => {
                    if let Some(g) = st.guard.as_mut() {
                        g.push(&chunk);
                    }
                    Some((Ok(chunk), st))
                }
                Some(Err(e)) => {
                    st.inner = None;
                    drop(st.guard.take()); // Drop::flush backfills the partial body
                    Some((Err(e), st))
                }
                None => {
                    st.inner = None;
                    drop(st.guard.take()); // normal EOF: flush
                    None
                }
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::client::ClientError;
    use bytes::Bytes;
    use futures_util::StreamExt;

    /// A decoder that uppercases each chunk — proves `channel_decode_stream`
    /// runs the channel decoder over the raw upstream bytes (before any
    /// protocol transform).
    struct Upper;
    impl ChannelStreamDecoder for Upper {
        fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
            chunk.to_ascii_uppercase()
        }
        fn finish(&mut self) -> Vec<u8> {
            b"!".to_vec()
        }
    }

    #[tokio::test]
    async fn channel_decode_stream_splice_runs_first() {
        let chunks: Vec<Result<Bytes, ClientError>> =
            vec![Ok(Bytes::from("ab")), Ok(Bytes::from("cd"))];
        let src: RespStream = Box::pin(futures_util::stream::iter(chunks));
        let out: Vec<Bytes> = channel_decode_stream(src, Box::new(Upper))
            .map(|r| r.unwrap())
            .collect()
            .await;
        let joined: Vec<u8> = out.concat();
        assert_eq!(joined, b"ABCD!");
    }
}
