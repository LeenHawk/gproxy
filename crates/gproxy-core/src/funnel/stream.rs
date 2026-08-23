use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use gproxy_channel_api::{BoxFuture, StreamDecoder, StreamEnd, TransportError};
use gproxy_protocol::SettleMode;

use crate::Shared;
use crate::host::Host;
use crate::usage::Ended;

use super::{FunnelCtx, complete_stream};

pub(crate) struct FunnelStream<H: Host> {
    upstream: crate::boundary::ByteStream,
    decoder: Option<Box<dyn StreamDecoder>>,
    pending: VecDeque<Bytes>,
    host: Option<Shared<H>>,
    ctx: Option<FunnelCtx>,
    status: http::StatusCode,
    state: State,
    ended: Option<Ended>,
    tail_usage: Option<gproxy_channel_api::NormalizedUsage>,
    output_chars: u64,
    terminal_error: Option<TransportError>,
}

enum State {
    Relaying,
    Draining,
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
            ended: None,
            tail_usage: None,
            output_chars: 0,
            terminal_error: None,
        }
    }

    fn finish_relay(&mut self, ended: Ended, error: Option<TransportError>) {
        if let Some(mut decoder) = self.decoder.take() {
            let end = match ended {
                Ended::Complete => StreamEnd::Complete,
                Ended::Interrupted => StreamEnd::Interrupted,
            };
            let tail = match decoder.finish(end) {
                Ok(tail) => tail,
                Err(error) => {
                    self.abort_relay(TransportError::Interrupted(error.to_string()));
                    return;
                }
            };
            self.pending
                .extend(tail.frames.into_iter().map(|frame| frame.0));
            self.tail_usage = tail.usage;
        }
        self.ended = Some(ended);
        self.terminal_error = error;
        self.state = State::Draining;
    }

    fn abort_relay(&mut self, error: TransportError) {
        self.decoder = None;
        self.ended = Some(Ended::Interrupted);
        self.terminal_error = Some(error);
        self.state = State::Draining;
    }

    fn begin_settle(&mut self) {
        let host = self.host.take().expect("stream host is present");
        let ctx = self.ctx.take().expect("stream funnel context is present");
        let ended = self.ended.take().expect("stream end is present");
        let usage = matches!(ctx.settle, SettleMode::OnResponse)
            .then(|| self.tail_usage.take())
            .flatten();
        let future: BoxFuture<'static, ()> = Box::pin(complete_stream(
            host.clone(),
            ctx,
            self.status,
            usage,
            Some(self.output_chars),
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
                        this.output_chars = this
                            .output_chars
                            .saturating_add(super::settlement::utf8_chars(&chunk));
                        let Some(decoder) = this.decoder.as_mut() else {
                            return Poll::Ready(Some(Ok(chunk)));
                        };
                        match decoder.push(chunk) {
                            Ok(frames) => {
                                this.pending.extend(frames.into_iter().map(|frame| frame.0))
                            }
                            Err(error) => {
                                this.abort_relay(TransportError::Interrupted(error.to_string()))
                            }
                        }
                    }
                    Poll::Ready(Some(Err(error))) => {
                        this.finish_relay(Ended::Interrupted, Some(error));
                    }
                    Poll::Ready(None) => this.finish_relay(Ended::Complete, None),
                },
                State::Draining => this.begin_settle(),
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
        if matches!(self.state, State::Settling(_) | State::Done) {
            return;
        }
        let (Some(host), Some(ctx)) = (self.host.take(), self.ctx.take()) else {
            return;
        };
        let usage = if matches!(self.state, State::Relaying) {
            self.decoder.take().and_then(|mut decoder| {
                matches!(ctx.settle, SettleMode::OnResponse)
                    .then(|| decoder.finish(StreamEnd::Interrupted).ok()?.usage)
                    .flatten()
            })
        } else {
            matches!(ctx.settle, SettleMode::OnResponse)
                .then(|| self.tail_usage.take())
                .flatten()
        };
        let ended = if self.pending.is_empty() {
            self.ended.take().unwrap_or(Ended::Interrupted)
        } else {
            Ended::Interrupted
        };
        if let Some(spawner) = host.spawner() {
            spawner.spawn(Box::pin(complete_stream(
                host.clone(),
                ctx,
                self.status,
                usage,
                Some(self.output_chars),
                ended,
            )));
        } else {
            tracing::warn!(
                request_id = %ctx.request_id,
                "stream dropped before inline settlement could complete"
            );
        }
    }
}
