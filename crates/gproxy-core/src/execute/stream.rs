//! The response body handed back from an exchange: one pass over the upstream
//! bytes that feeds capture, usage observation, optional rewriting and the
//! caller, enforcing the read cap, idle timeout and cancellation. Dropping it
//! early still finishes the exchange (and the request, if terminal).

use super::Exchange;
use crate::{
    CaptureEvent,
    api::lifecycle::now_ms,
    rewrite::{StreamRewriter, apply_body},
};
use futures_util::StreamExt;
use gproxy_channel::channel::{ResponseReason, UsageStreamEnd};
use gproxy_protocol::{
    HttpBody,
    connection::{ByteStream, Bytes, StreamFraming, TransportError},
};
use std::sync::Arc;

struct Guard {
    exchange: Arc<Exchange>,
    finished: bool,
    status: http::StatusCode,
    headers: http::HeaderMap,
    /// Whole response so far, when the channel's extractor needs it.
    accumulated: Option<Vec<u8>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if !self.finished {
            // A conversion may let go of a native body right after its terminal
            // event; what was read is still the usage evidence.
            let accumulated = self.accumulated.take();
            self.exchange.clone().finish_detached(
                UsageStreamEnd::Interrupted,
                self.status,
                std::mem::take(&mut self.headers),
                accumulated,
                now_ms(),
            );
        }
    }
}

enum Rewrite {
    None,
    Stream(StreamRewriter),
    /// Whole-body rules: hold everything, rewrite once at EOF.
    Whole(Vec<u8>),
}

struct State {
    inner: Option<ByteStream>,
    guard: Guard,
    rewrite: Rewrite,
    read: u64,
    /// Bytes ready to yield before touching the inner stream again.
    pending: Vec<Bytes>,
    ended: bool,
}

fn transport_error(message: &str) -> TransportError {
    Box::new(std::io::Error::other(message.to_owned()))
}

pub(crate) fn observed_body(
    exchange: Arc<Exchange>,
    body: HttpBody,
    framing: Option<StreamFraming>,
    status: http::StatusCode,
    headers: http::HeaderMap,
) -> ByteStream {
    let inner: ByteStream = match body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::iter([Ok(bytes)])),
        HttpBody::Stream(stream) => stream,
    };
    let rewrite = if exchange.response_rules.is_empty() {
        Rewrite::None
    } else {
        match framing {
            Some(framing) => Rewrite::Stream(StreamRewriter::new(
                exchange.response_rules.clone(),
                framing,
                exchange.limits.read_bytes,
            )),
            None => Rewrite::Whole(Vec::new()),
        }
    };
    let accumulated = exchange.wants_accumulated_response().then(Vec::new);
    let state = State {
        inner: Some(inner),
        guard: Guard {
            exchange,
            finished: false,
            status,
            headers,
            accumulated,
        },
        rewrite,
        read: 0,
        pending: Vec::new(),
        ended: false,
    };
    Box::pin(futures_util::stream::unfold(state, step))
}

async fn step(mut state: State) -> Option<(Result<Bytes, TransportError>, State)> {
    loop {
        if !state.pending.is_empty() {
            let chunk = state.pending.remove(0);
            return Some((Ok(chunk), state));
        }
        if state.ended {
            return None;
        }
        let exchange = state.guard.exchange.clone();
        let inner = state.inner.as_mut()?;
        let cancellation = exchange.context.attempt.request.cancellation.clone();
        let next = tokio::select! {
            biased;
            () = cancellation.cancelled() => { exchange.set_reason(ResponseReason::Cancelled); Err(transport_error("request cancelled")) },
            next = crate::rt::timeout(exchange.limits.stream_idle, inner.next()) => match next {
                Some(item) => Ok(item),
                None => { exchange.set_reason(ResponseReason::Timeout); Err(transport_error("upstream stream idle timeout")) },
            },
        };
        match next {
            Err(error) => return Some((Err(error), end(state, UsageStreamEnd::Interrupted).await)),
            Ok(Some(Err(error))) => {
                exchange.set_reason(ResponseReason::ConnectionError);
                return Some((Err(error), end(state, UsageStreamEnd::Interrupted).await));
            }
            Ok(None) => {
                let mut tail = Vec::new();
                match &mut state.rewrite {
                    Rewrite::None => {}
                    Rewrite::Stream(rewriter) => match rewriter.finish() {
                        Ok(bytes) if !bytes.is_empty() => tail.push(Bytes::from(bytes)),
                        Ok(_) => {}
                        Err(error) => {
                            let error = transport_error(&error.to_string());
                            return Some((
                                Err(error),
                                end(state, UsageStreamEnd::Interrupted).await,
                            ));
                        }
                    },
                    Rewrite::Whole(buffer) => {
                        let buffer = std::mem::take(buffer);
                        match apply_body(&exchange.response_rules, &buffer) {
                            Ok(Some(rewritten)) => tail.push(Bytes::from(rewritten)),
                            Ok(None) => tail.push(Bytes::from(buffer)),
                            Err(error) => {
                                let error = transport_error(&error.to_string());
                                return Some((
                                    Err(error),
                                    end(state, UsageStreamEnd::Interrupted).await,
                                ));
                            }
                        }
                    }
                }
                state.pending = tail;
                state = end(state, UsageStreamEnd::Complete).await;
                continue;
            }
            Ok(Some(Ok(chunk))) => {
                state.read += chunk.len() as u64;
                if state.read > exchange.limits.read_bytes {
                    let error = transport_error("upstream response exceeds the read limit");
                    return Some((Err(error), end(state, UsageStreamEnd::Interrupted).await));
                }
                if exchange.wants_full_capture() {
                    exchange.record(CaptureEvent::ResponseChunk(&chunk));
                }
                exchange.observe_chunk(&chunk);
                if let Some(buffer) = state.guard.accumulated.as_mut() {
                    buffer.extend_from_slice(&chunk);
                }
                match &mut state.rewrite {
                    Rewrite::None => return Some((Ok(chunk), state)),
                    Rewrite::Stream(rewriter) => match rewriter.push(&chunk) {
                        Ok(out) if out.is_empty() => continue,
                        Ok(out) => return Some((Ok(Bytes::from(out)), state)),
                        Err(error) => {
                            let error = transport_error(&error.to_string());
                            return Some((
                                Err(error),
                                end(state, UsageStreamEnd::Interrupted).await,
                            ));
                        }
                    },
                    Rewrite::Whole(buffer) => {
                        buffer.extend_from_slice(&chunk);
                        continue;
                    }
                }
            }
        }
    }
}

async fn end(mut state: State, how: UsageStreamEnd) -> State {
    state.ended = true;
    state.inner = None;
    if !state.guard.finished {
        state.guard.finished = true;
        let accumulated = state.guard.accumulated.take();
        state
            .guard
            .exchange
            .finish(
                how,
                Some(state.guard.status),
                Some(&state.guard.headers),
                accumulated.as_deref(),
                now_ms(),
            )
            .await;
    }
    state
}

/// Wrap a converted client stream so the request settles exactly once when it
/// ends (Completed), errors (Failed) or is dropped (Cancelled). The upstream
/// exchanges behind it are observed by AttemptUpstream already.
pub(crate) fn settling(
    funnel: Arc<super::Funnel>,
    inner: ByteStream,
    completed: crate::UsageState,
) -> ByteStream {
    struct Guard {
        funnel: Arc<super::Funnel>,
        done: bool,
        completed: crate::UsageState,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            if !self.done {
                self.funnel
                    .clone()
                    .finish_detached(crate::UsageState::Cancelled);
            }
        }
    }
    struct State {
        inner: Option<ByteStream>,
        guard: Guard,
    }
    Box::pin(futures_util::stream::unfold(
        State {
            inner: Some(inner),
            guard: Guard {
                funnel,
                done: false,
                completed,
            },
        },
        |mut state| async move {
            let inner = state.inner.as_mut()?;
            match inner.next().await {
                Some(Ok(chunk)) => Some((Ok(chunk), state)),
                Some(Err(error)) => {
                    state.inner = None;
                    state.guard.done = true;
                    state
                        .guard
                        .funnel
                        .finish(state.guard.funnel.interrupted_state())
                        .await;
                    Some((Err(error), state))
                }
                None => {
                    state.inner = None;
                    state.guard.done = true;
                    state.guard.funnel.finish(state.guard.completed).await;
                    None
                }
            }
        },
    ))
}

/// Consume superseded rejection bodies through their observation wrapper.
pub(super) async fn drain(body: HttpBody, timeout: std::time::Duration) {
    if let HttpBody::Stream(mut stream) = body {
        let _ = crate::rt::timeout(timeout, async {
            while let Some(chunk) = stream.next().await {
                if chunk.is_err() {
                    break;
                }
            }
        })
        .await;
    }
}
