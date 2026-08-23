use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use gproxy_channel_api::{BoxFuture, StreamDecoder, TransportError};
use gproxy_protocol::SettleMode;

use crate::Shared;
use crate::funnel::{FunnelCtx, complete_stream};
use crate::host::Host;
use crate::usage::Ended;

pub(crate) struct FunnelStream<H: Host> {
    upstream: crate::boundary::ByteStream,
    decoder: Option<Box<dyn StreamDecoder>>,
    pending: VecDeque<Bytes>,
    host: Option<Shared<H>>,
    ctx: Option<FunnelCtx>,
    status: http::StatusCode,
    state: State,
    terminal_error: Option<TransportError>,
}

enum State {
    Relaying,
    Settling(BoxFuture<'static, ()>),
    Done,
}

impl<H: Host> FunnelStream<H> {
    pub(crate) fn new(
        upstream: crate::boundary::ByteStream,
        decoder: Option<Box<dyn StreamDecoder>>,
        host: Shared<H>,
        ctx: FunnelCtx,
        status: http::StatusCode,
    ) -> Self {
        Self {
            upstream,
            decoder,
            pending: VecDeque::new(),
            host: Some(host),
            ctx: Some(ctx),
            status,
            state: State::Relaying,
            terminal_error: None,
        }
    }

    fn begin_settle(&mut self, ended: Ended, error: Option<TransportError>) {
        let host = self.host.take().expect("stream host is present");
        let ctx = self.ctx.take().expect("stream funnel context is present");
        let usage = self.decoder.take().and_then(|mut decoder| {
            let usage = decoder.finish().usage;
            matches!(ctx.settle, SettleMode::OnResponse)
                .then_some(usage)
                .flatten()
        });
        self.terminal_error = error;
        let future: BoxFuture<'static, ()> = Box::pin(complete_stream(
            host.clone(),
            ctx,
            self.status,
            usage,
            ended,
        ));
        if let Some(spawner) = host.spawner() {
            spawner.spawn(future);
            self.state = State::Done;
        } else {
            self.state = State::Settling(future);
        }
    }

    fn poll_terminal(&mut self) -> Poll<Option<Result<Bytes, TransportError>>> {
        Poll::Ready(self.terminal_error.take().map(Err))
    }
}

impl<H: Host> Stream for FunnelStream<H> {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(frame) = this.pending.pop_front() {
                return Poll::Ready(Some(Ok(frame)));
            }
            match &mut this.state {
                State::Relaying => match this.upstream.as_mut().poll_next(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Some(Ok(chunk))) => {
                        let Some(decoder) = this.decoder.as_mut() else {
                            return Poll::Ready(Some(Ok(chunk)));
                        };
                        match decoder.push(&chunk) {
                            Ok(frames) => {
                                this.pending.extend(frames.into_iter().map(|frame| frame.0))
                            }
                            Err(error) => this.begin_settle(
                                Ended::Interrupted,
                                Some(TransportError::Interrupted(error.to_string())),
                            ),
                        }
                    }
                    Poll::Ready(Some(Err(error))) => {
                        this.begin_settle(Ended::Interrupted, Some(error));
                    }
                    Poll::Ready(None) => this.begin_settle(Ended::Complete, None),
                },
                State::Settling(future) => match future.as_mut().poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(()) => {
                        this.state = State::Done;
                    }
                },
                State::Done => return this.poll_terminal(),
            }
        }
    }
}

impl<H: Host> Drop for FunnelStream<H> {
    fn drop(&mut self) {
        if !matches!(self.state, State::Relaying) {
            return;
        }
        let (Some(host), Some(ctx)) = (self.host.take(), self.ctx.take()) else {
            return;
        };
        let usage = self.decoder.take().and_then(|mut decoder| {
            matches!(ctx.settle, SettleMode::OnResponse)
                .then(|| decoder.finish().usage)
                .flatten()
        });
        if let Some(spawner) = host.spawner() {
            spawner.spawn(Box::pin(complete_stream(
                host.clone(),
                ctx,
                self.status,
                usage,
                Ended::Interrupted,
            )));
        } else {
            tracing::warn!(
                request_id = %ctx.request_id,
                "stream dropped before inline settlement could complete"
            );
        }
    }
}
