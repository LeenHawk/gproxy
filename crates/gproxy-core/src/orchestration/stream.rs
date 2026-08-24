use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use gproxy_channel_api::{Frame, OperationStream, PreparedRequest, StreamEnd, TransportError};

use crate::Shared;
use crate::boundary::ByteStream;
use crate::continuation::{Continuation, ContinuationKey};
use crate::control::Target;
use crate::host::Host;

pub(super) struct Scope {
    pub channel: &'static str,
    pub owner_user_id: i64,
    pub target: Target,
    pub generation: String,
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub upstream_url: String,
    pub cleanup: PreparedRequest,
    pub ttl_secs: u64,
}

pub(super) fn wrap<H: Host>(
    host: Shared<H>,
    upstream: ByteStream,
    pending: VecDeque<Bytes>,
    codec: Box<dyn OperationStream>,
    scope: Scope,
) -> ByteStream {
    Box::pin(ContinuationStream {
        host,
        upstream: Some(upstream),
        input: pending,
        output: VecDeque::new(),
        codec: Some(codec),
        scope: Some(scope),
        terminal_error: None,
        done: false,
    })
}

struct ContinuationStream<H: Host> {
    host: Shared<H>,
    upstream: Option<ByteStream>,
    input: VecDeque<Bytes>,
    output: VecDeque<Bytes>,
    codec: Option<Box<dyn OperationStream>>,
    scope: Option<Scope>,
    terminal_error: Option<TransportError>,
    done: bool,
}

impl<H: Host> ContinuationStream<H> {
    fn push(&mut self, chunk: Bytes) {
        let result = self
            .codec
            .as_mut()
            .expect("continuation codec is present")
            .push(chunk);
        match result {
            Ok(result) => {
                self.output
                    .extend(result.frames.into_iter().map(|frame| frame.0));
                if let Some(pause) = result.pause {
                    self.park(pause);
                }
            }
            Err(error) => self.interrupt(TransportError::Interrupted(error.to_string())),
        }
    }

    fn park(&mut self, pause: gproxy_channel_api::Pause) {
        let Some(scope) = self.scope.take() else {
            return;
        };
        let key = ContinuationKey {
            channel: scope.channel,
            provider_id: scope.target.provider.id,
            owner_user_id: scope.owner_user_id,
            id: pause.id,
        };
        let continuation = Continuation {
            key: key.clone(),
            generation: scope.generation.clone(),
            target: scope.target,
            stream: self.upstream.take().expect("upstream stream is present"),
            pending: pause.pending.into_iter().collect(),
            status: scope.status,
            headers: scope.headers,
            state: pause.state,
            cleanup: scope.cleanup,
            upstream_url: scope.upstream_url,
        };
        let stored = self
            .host
            .continuations()
            .expect("continuation capability was checked")
            .put(continuation);
        match stored {
            Ok(replaced) => {
                if let Some(replaced) = replaced {
                    super::cleanup::spawn_continuation(self.host.clone(), replaced);
                }
                super::cleanup::schedule_expiry(
                    self.host.clone(),
                    key,
                    scope.generation,
                    scope.ttl_secs,
                );
                self.done = true;
                self.codec.take();
            }
            Err((error, continuation)) => {
                super::cleanup::spawn_continuation(self.host.clone(), *continuation);
                self.interrupt(TransportError::Interrupted(error.to_string()));
            }
        }
    }

    fn finish(&mut self, end: StreamEnd) {
        if let Some(mut codec) = self.codec.take() {
            match codec.finish(end) {
                Ok(frames) => self
                    .output
                    .extend(frames.into_iter().map(|frame: Frame| frame.0)),
                Err(error) => {
                    self.terminal_error = Some(TransportError::Interrupted(error.to_string()))
                }
            }
        }
        self.spawn_cleanup();
        self.done = true;
    }

    fn interrupt(&mut self, error: TransportError) {
        self.terminal_error = Some(error);
        self.finish(StreamEnd::Interrupted);
    }

    fn spawn_cleanup(&mut self) {
        if let Some(scope) = self.scope.take() {
            super::cleanup::spawn_request(
                self.host.clone(),
                scope.target,
                format!("{}:cleanup", scope.generation),
                scope.cleanup,
            );
        }
    }
}

impl<H: Host> Stream for ContinuationStream<H> {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(output) = this.output.pop_front() {
                return Poll::Ready(Some(Ok(output)));
            }
            if this.done {
                return Poll::Ready(this.terminal_error.take().map(Err));
            }
            if let Some(input) = this.input.pop_front() {
                this.push(input);
                continue;
            }
            match this
                .upstream
                .as_mut()
                .expect("upstream stream is present")
                .as_mut()
                .poll_next(cx)
            {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(chunk))) => this.push(chunk),
                Poll::Ready(Some(Err(error))) => this.interrupt(error),
                Poll::Ready(None) => this.finish(StreamEnd::Complete),
            }
        }
    }
}

impl<H: Host> Drop for ContinuationStream<H> {
    fn drop(&mut self) {
        if !self.done {
            self.spawn_cleanup();
        }
    }
}
