//! Streaming response tail (§6.4, D4). Native-only. Holds ONLY the body-side
//! conversion invoked by `failover` when materializing a streaming attempt — it
//! does not iterate candidates or call `classify`.

use crate::channel::ChannelStreamDecoder;
use crate::http::client::RespStream;
use crate::pipeline::outcome::ByteStream;
use crate::pipeline::settle::StreamGuard;
use crate::transform::stream_adapter::SseTransformer;

const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyntheticTransport {
    Sse,
    GeminiJson,
}

pub fn synthetic_transport(ctx: &crate::pipeline::context::RequestCtx) -> SyntheticTransport {
    if matches!(
        ctx.op.map(|op| op.kind),
        Some(crate::protocol::OperationKind::ContentGeneration(
            crate::protocol::ContentGenerationKind::GeminiGenerateContent
        ))
    ) && !ctx
        .query
        .as_deref()
        .is_some_and(|query| query.split('&').any(|part| part == "alt=sse"))
    {
        SyntheticTransport::GeminiJson
    } else {
        SyntheticTransport::Sse
    }
}

/// Return the downstream stream immediately while the normal failover loop runs
/// in a background task against a non-streaming target. The task owns all
/// accounting work, so client disconnect does not leak pending quota charges.
pub fn synthetic_outcome(
    state: crate::app::AppState,
    ctx: crate::pipeline::context::RequestCtx,
    candidates: Vec<crate::pipeline::context::Candidate>,
    quota_scopes: Vec<(crate::store::persistence::records::Scope, i64)>,
    pending_micros: i64,
) -> crate::pipeline::outcome::ExecOutcome {
    use http::{HeaderMap, HeaderValue, StatusCode, header};
    use tokio::sync::mpsc;

    let kind = match ctx.op.expect("classified").kind {
        crate::protocol::OperationKind::ContentGeneration(kind) => kind,
        crate::protocol::OperationKind::Provider(_) => unreachable!("synthetic content stream"),
    };
    let transport = synthetic_transport(&ctx);
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        let work = crate::pipeline::failover::run_failover(&state, &ctx, &candidates);
        tokio::pin!(work);
        let mut interval = tokio::time::interval(KEEPALIVE_INTERVAL);
        interval.tick().await;
        let result = loop {
            tokio::select! {
                result = &mut work => break result,
                _ = interval.tick() => {
                    let _ = tx.try_send(Ok(synthetic_keepalive(kind, transport)));
                }
            }
        };

        if !matches!(&result, Ok(outcome) if outcome.status.is_success()) {
            crate::billing::pending::refund(state.cache.as_ref(), &quota_scopes, pending_micros)
                .await;
        }

        match result {
            Ok(outcome) if outcome.status.is_success() => match outcome.body {
                crate::pipeline::outcome::ResponseBody::Full(body) => {
                    let bytes = synthetic_final(kind, transport, &body).unwrap_or_else(|error| {
                        tracing::warn!(error = %error, "failed to synthesize response stream");
                        synthetic_error(kind, transport)
                    });
                    let _ = tx.send(Ok(bytes)).await;
                }
                crate::pipeline::outcome::ResponseBody::Stream(mut stream) => {
                    use futures_util::StreamExt;
                    while let Some(item) = stream.next().await {
                        if tx.send(item).await.is_err() {
                            break;
                        }
                    }
                }
            },
            Ok(_) | Err(_) => {
                let _ = tx.send(Ok(synthetic_error(kind, transport))).await;
            }
        }
    });

    let stream = futures_util::stream::unfold(rx, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    });
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        match transport {
            SyntheticTransport::Sse => HeaderValue::from_static("text/event-stream"),
            SyntheticTransport::GeminiJson => HeaderValue::from_static("application/json"),
        },
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    crate::pipeline::outcome::ExecOutcome {
        status: StatusCode::OK,
        headers,
        body: crate::pipeline::outcome::ResponseBody::Stream(Box::pin(stream)),
        disposition: crate::channel::Disposition::Success,
    }
}

fn synthetic_keepalive(
    kind: crate::protocol::ContentGenerationKind,
    transport: SyntheticTransport,
) -> bytes::Bytes {
    if transport == SyntheticTransport::GeminiJson {
        return bytes::Bytes::from_static(b"\n");
    }
    if kind == crate::protocol::ContentGenerationKind::ClaudeMessages {
        return bytes::Bytes::from_static(b"event: ping\ndata: {\"type\":\"ping\"}\n\n");
    }
    bytes::Bytes::from_static(b": keep-alive\n\n")
}

fn synthetic_final(
    kind: crate::protocol::ContentGenerationKind,
    transport: SyntheticTransport,
    body: &[u8],
) -> Result<bytes::Bytes, crate::transform::TransformError> {
    if transport == SyntheticTransport::GeminiJson {
        let value: serde_json::Value = serde_json::from_slice(body).map_err(|error| {
            crate::transform::TransformError::InvalidInput {
                reason: format!("synthetic gemini response is not JSON: {error}"),
            }
        })?;
        return serde_json::to_vec(&vec![value])
            .map(bytes::Bytes::from)
            .map_err(|error| crate::transform::TransformError::Serialization {
                reason: error.to_string(),
            });
    }
    crate::transform::stream_adapter::synthesize_sse(kind, body).map(bytes::Bytes::from)
}

fn synthetic_error(
    kind: crate::protocol::ContentGenerationKind,
    transport: SyntheticTransport,
) -> bytes::Bytes {
    if transport == SyntheticTransport::GeminiJson {
        return bytes::Bytes::from_static(
            b"[{\"error\":{\"code\":502,\"status\":\"UNAVAILABLE\",\"message\":\"upstream request failed\"}}]",
        );
    }
    crate::pipeline::settle::frames::error_frame(kind)
}

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

/// Tee provider-native chunks into the §17 settlement guard while passing them
/// through unchanged. This is spliced after channel shaping/response rules and
/// before any protocol transform, so usage is extracted from upstream semantics.
pub fn instrument_settle_stream(s: ByteStream, guard: StreamGuard) -> ByteStream {
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
                    tracing::warn!(error = %e, "upstream stream failed");
                    drop(st.guard.take()); // Drop settles Interrupted
                    Some((Err(e), st))
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

/// Convert a mid-stream transport error into one protocol-shaped downstream
/// error frame. Usage settlement is handled earlier on the provider-native
/// stream by [`instrument_settle_stream`].
pub fn instrument_error_frame(
    s: ByteStream,
    kind: crate::protocol::ContentGenerationKind,
) -> ByteStream {
    use futures_util::StreamExt;

    struct State {
        inner: Option<ByteStream>,
        kind: crate::protocol::ContentGenerationKind,
    }

    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(s),
            kind,
        },
        |mut st| async move {
            let inner = st.inner.as_mut()?;
            match inner.next().await {
                Some(Ok(chunk)) => Some((Ok(chunk), st)),
                Some(Err(e)) => {
                    st.inner = None;
                    tracing::warn!(error = %e, "upstream stream failed");
                    Some((
                        Ok(crate::pipeline::settle::frames::error_frame(st.kind)),
                        st,
                    ))
                }
                None => None,
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

    #[test]
    fn synthetic_keepalive_is_protocol_appropriate() {
        use crate::protocol::ContentGenerationKind as K;

        assert_eq!(
            synthetic_keepalive(K::ClaudeMessages, SyntheticTransport::Sse),
            bytes::Bytes::from_static(b"event: ping\ndata: {\"type\":\"ping\"}\n\n")
        );
        assert_eq!(
            synthetic_keepalive(K::OpenAiResponses, SyntheticTransport::Sse),
            bytes::Bytes::from_static(b": keep-alive\n\n")
        );
        assert_eq!(
            synthetic_keepalive(K::GeminiGenerateContent, SyntheticTransport::GeminiJson),
            bytes::Bytes::from_static(b"\n")
        );
    }
}
