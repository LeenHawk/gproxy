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
use gproxy_channel::channel::UsageStreamEnd;
use gproxy_protocol::{
    HttpBody,
    connection::{ByteStream, Bytes, StreamFraming, TransportError},
};
use std::sync::Arc;

struct Guard {
    exchange: Arc<Exchange>,
    finished: bool,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if !self.finished {
            self.exchange
                .clone()
                .finish_detached(UsageStreamEnd::Interrupted, now_ms());
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
    accumulated: Option<Vec<u8>>,
    read: u64,
    status: http::StatusCode,
    headers: http::HeaderMap,
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
        },
        rewrite,
        accumulated,
        read: 0,
        status,
        headers,
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
            () = cancellation.cancelled() => Err(transport_error("request cancelled")),
            next = tokio::time::timeout(exchange.limits.stream_idle, inner.next()) => match next {
                Ok(item) => Ok(item),
                Err(_) => Err(transport_error("upstream stream idle timeout")),
            },
        };
        match next {
            Err(error) => return Some((Err(error), end(state, UsageStreamEnd::Interrupted).await)),
            Ok(Some(Err(error))) => {
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
                if let Some(buffer) = state.accumulated.as_mut() {
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
        let accumulated = state.accumulated.take();
        state
            .guard
            .exchange
            .finish(
                how,
                Some(state.status),
                Some(&state.headers),
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
pub(crate) fn settling(funnel: Arc<super::Funnel>, inner: ByteStream) -> ByteStream {
    struct Guard {
        funnel: Arc<super::Funnel>,
        done: bool,
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
            },
        },
        |mut state| async move {
            let inner = state.inner.as_mut()?;
            match inner.next().await {
                Some(Ok(chunk)) => Some((Ok(chunk), state)),
                Some(Err(error)) => {
                    state.inner = None;
                    state.guard.done = true;
                    state.guard.funnel.finish(crate::UsageState::Failed).await;
                    Some((Err(error), state))
                }
                None => {
                    state.inner = None;
                    state.guard.done = true;
                    state
                        .guard
                        .funnel
                        .finish(crate::UsageState::Completed)
                        .await;
                    None
                }
            }
        },
    ))
}
