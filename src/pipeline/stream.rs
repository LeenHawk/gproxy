//! Streaming response tail (§6.4, D4): body-side conversion invoked by
//! `failover`; it does not iterate candidates or call `classify`.

use crate::http::client::{ByteStreamDecoder, RespStream};
use crate::pipeline::outcome::ByteStream;
use crate::pipeline::settle::StreamGuard;
use crate::transform::stream_adapter::SseTransformer;
#[cfg(not(target_arch = "wasm32"))]
use tracing::Instrument as _;

mod raw_capture;
pub use raw_capture::{RawCaptureGuard, capture_raw_stream};

#[cfg(not(target_arch = "wasm32"))]
const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyntheticTransport {
    Sse,
    GeminiJson,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn synthetic_transport(ctx: &crate::pipeline::context::RequestCtx) -> SyntheticTransport {
    if matches!(
        ctx.op.map(|op| op.kind()),
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
#[cfg(not(target_arch = "wasm32"))]
pub fn synthetic_outcome(
    state: crate::app::AppState,
    ctx: crate::pipeline::context::RequestCtx,
    candidates: Vec<crate::pipeline::context::Candidate>,
    quota_scopes: Vec<(crate::store::persistence::records::Scope, i64)>,
    pending_micros: i64,
) -> crate::pipeline::outcome::ExecOutcome {
    use http::{HeaderMap, HeaderValue, StatusCode, header};
    use tokio::sync::mpsc;

    let kind = match ctx.op.expect("classified").kind() {
        crate::protocol::OperationKind::ContentGeneration(kind) => kind,
        crate::protocol::OperationKind::Provider(_) => unreachable!("synthetic content stream"),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    let transport = synthetic_transport(&ctx);
    let request_id = ctx.request_id.clone();
    let (tx, rx) = mpsc::channel(8);
    let span = tracing::Span::current();
    tokio::spawn(
        async move {
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
                crate::billing::pending::refund(
                    state.cache.as_ref(),
                    &quota_scopes,
                    pending_micros,
                    &ctx.request_id,
                )
                .await;
            }

            match result {
                Ok(outcome) if outcome.status.is_success() => match outcome.body {
                    crate::pipeline::outcome::ResponseBody::Full(body) => {
                        let bytes =
                            synthetic_final(kind, transport, &body).unwrap_or_else(|error| {
                                tracing::warn!(
                                    request_id = %request_id,
                                    error = %error,
                                    "failed to synthesize response stream"
                                );
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
                Ok(_) => {
                    let _ = tx.send(Ok(synthetic_error(kind, transport))).await;
                }
                Err(error) => {
                    let error =
                        crate::http::telemetry::redact_url_query(&error.to_string()).into_owned();
                    tracing::warn!(
                        request_id = %request_id,
                        error = %error,
                        "synthetic stream pipeline failed"
                    );
                    let _ = tx.send(Ok(synthetic_error(kind, transport))).await;
                }
            }
        }
        .instrument(span),
    );

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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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
                        let out = match st.t.push_detailed(&chunk) {
                            Ok(out) => out,
                            Err(error) => {
                                st.inner = None;
                                return Some((
                                    Err(crate::http::client::ClientError::Transport(
                                        error.to_string(),
                                    )),
                                    st,
                                ));
                            }
                        };
                        crate::pipeline::transform::log_diagnostics(&out.diagnostics);
                        let out = out.value;
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
                        let tail = match st.t.finish_detailed() {
                            Ok(tail) => tail,
                            Err(error) => {
                                return Some((
                                    Err(crate::http::client::ClientError::Transport(
                                        error.to_string(),
                                    )),
                                    st,
                                ));
                            }
                        };
                        crate::pipeline::transform::log_diagnostics(&tail.diagnostics);
                        let tail = tail.value;
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
/// [`ByteStreamDecoder`] exactly like [`transform_byte_stream`] drives an
/// `SseTransformer`: `push` per upstream chunk, `finish` at EOF; upstream errors
/// are forwarded once and end the stream. Its `ByteStream` output is then fed to
/// either the M2 transform ([`transform_byte_stream`]) or straight to the client
/// ([`into_byte_stream`]) by the caller.
pub fn channel_decode_stream(s: RespStream, decoder: Box<dyn ByteStreamDecoder>) -> ByteStream {
    use bytes::Bytes;
    use futures_util::StreamExt;

    struct State {
        inner: Option<RespStream>,
        decoder: Box<dyn ByteStreamDecoder>,
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
                        let out = match st.decoder.push(&chunk) {
                            Ok(out) => out,
                            Err(error) => {
                                st.inner = None;
                                return Some((Err(error), st));
                            }
                        };
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
                        let tail = match st.decoder.finish() {
                            Ok(tail) => tail,
                            Err(error) => return Some((Err(error), st)),
                        };
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
        request_id: String,
    }

    let request_id = guard.request_id().to_owned();
    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(s),
            guard: Some(guard),
            request_id,
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
                    tracing::warn!(
                        request_id = %st.request_id,
                        error = %e,
                        "upstream stream failed"
                    );
                    drop(st.guard.take()); // Drop settles Interrupted
                    Some((Err(e), st))
                }
                None => {
                    st.inner = None;
                    if let Some(g) = st.guard.take() {
                        finish_stream_guard(g).await;
                    }
                    None
                }
            }
        },
    ))
}

#[cfg(not(target_arch = "wasm32"))]
async fn finish_stream_guard(guard: StreamGuard) {
    guard.finish();
}

#[cfg(target_arch = "wasm32")]
async fn finish_stream_guard(guard: StreamGuard) {
    guard.finish().await;
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
                Some(Err(_)) => {
                    st.inner = None;
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
    impl ByteStreamDecoder for Upper {
        fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, ClientError> {
            Ok(chunk.to_ascii_uppercase())
        }
        fn finish(&mut self) -> Result<Vec<u8>, ClientError> {
            Ok(b"!".to_vec())
        }
    }

    struct RejectFinish;
    impl ByteStreamDecoder for RejectFinish {
        fn push(&mut self, _chunk: &[u8]) -> Result<Vec<u8>, ClientError> {
            Ok(Vec::new())
        }

        fn finish(&mut self) -> Result<Vec<u8>, ClientError> {
            Err(ClientError::Decode("truncated frame".to_owned()))
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

    #[tokio::test]
    async fn channel_decode_stream_propagates_finish_errors() {
        let src: RespStream = Box::pin(futures_util::stream::empty());
        let errors: Vec<ClientError> = channel_decode_stream(src, Box::new(RejectFinish))
            .filter_map(|item| async move { item.err() })
            .collect()
            .await;

        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], ClientError::Decode(reason) if reason == "truncated frame"));
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

    #[test]
    fn relay_buffer_bounds_one_oversized_chunk() {
        let mut buffer = crate::pipeline::settle::RelayBuffer::new();
        buffer.push(Bytes::from(vec![b'x'; 5 << 20]));
        let logged = buffer.concat_for_log();

        assert!(logged.len() < 5 << 20);
        assert!(String::from_utf8_lossy(&logged).starts_with("…[truncated:"));
    }
}
