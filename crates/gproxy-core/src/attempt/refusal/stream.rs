use std::collections::VecDeque;

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{StreamCtx, StreamEnd, TransportError};
use gproxy_protocol::{ContentGenerationKind as Kind, StreamFraming};
use serde_json::Value;

use crate::boundary::ByteStream;
use crate::error::CoreError;
use crate::host::Host;

use super::retry::Runner;

const MAX_BUFFER: usize = 100 * 1024 * 1024;

struct State<H: Host> {
    runner: Runner<H>,
    input: ByteStream,
    headers: http::HeaderMap,
    collector: Option<gproxy_transform::ResponseCollector>,
    buffered: Vec<Bytes>,
    raw: Vec<Bytes>,
    buffered_len: usize,
    pending: VecDeque<Bytes>,
    live: bool,
    done: bool,
}

pub(super) fn wrap<H: Host>(
    runner: Runner<H>,
    response: http::Response<ByteStream>,
) -> http::Response<ByteStream> {
    let (mut parts, input) = response.into_parts();
    let state = State {
        runner,
        input,
        headers: parts.headers.clone(),
        collector: None,
        buffered: Vec::new(),
        raw: Vec::new(),
        buffered_len: 0,
        pending: VecDeque::new(),
        live: false,
        done: false,
    }
    .start();
    let body = futures_util::stream::unfold(state, |mut state| async move {
        match state.next().await {
            Ok(Some(frame)) => Some((Ok(frame), state)),
            Ok(None) => None,
            Err(error) => {
                state.done = true;
                Some((Err(TransportError::Interrupted(error.to_string())), state))
            }
        }
    });
    parts.headers.remove(http::header::CONTENT_LENGTH);
    parts.headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/event-stream"),
    );
    http::Response::from_parts(parts, Box::pin(body))
}

impl<H: Host> State<H> {
    fn start(mut self) -> Self {
        let channel = self
            .runner
            .core
            .channels
            .get(&self.runner.facts.target.provider.channel)
            .expect("prepared channel");
        let decoder = channel.stream_decoder(StreamCtx {
            key: self.runner.facts.key.expect("Messages"),
            framing: self.runner.facts.target_framing,
            request_body: &self.runner.replay.body,
            response_headers: &self.headers,
        });
        self.runner.meter.start(
            decoder,
            self.runner.facts.target.upstream_model.clone(),
            self.runner.facts.upstream_started_at_ms.expect("send time"),
            self.runner.replay.body.clone(),
        );
        self.collector = Some(
            gproxy_transform::ResponseCollector::new(Kind::ClaudeMessages)
                .expect("Claude collector"),
        );
        self
    }

    async fn next(&mut self) -> Result<Option<Bytes>, CoreError> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Ok(Some(frame));
            }
            if self.done {
                return Ok(None);
            }
            match self.input.next().await {
                Some(Ok(chunk)) => {
                    if !self.live {
                        self.buffered_len = self.buffered_len.saturating_add(chunk.len());
                        if self.buffered_len > MAX_BUFFER {
                            return Err(CoreError::Transform(
                                "fallback response exceeds 100 MiB".into(),
                            ));
                        }
                        self.raw.push(chunk.clone());
                    }
                    for frame in self.runner.meter.push(chunk)? {
                        if self.live {
                            self.pending.push_back(frame.0);
                        } else {
                            let collector = self.collector.as_mut().expect("buffered collector");
                            collector.push(frame.0.clone())?;
                            self.buffered.push(frame.0);
                            // Anthropic/OR own mid-output fallback. Only pre-output refusals
                            // need a gateway retry; accepted output stays live and zero-copy.
                            if self.runner.sent == 1
                                && self.runner.replay.policy.capabilities.server_side
                                && collector.claude_has_output()
                            {
                                self.live = true;
                                self.collector = None;
                                self.raw.clear();
                                self.pending.extend(self.buffered.drain(..));
                            }
                        }
                    }
                }
                Some(Err(error)) => {
                    self.runner.meter.finish(StreamEnd::Interrupted, false)?;
                    return Err(error.into());
                }
                None => {
                    let tail = self.runner.meter.finish(StreamEnd::Complete, false)?;
                    if self.live {
                        self.pending.extend(tail.into_iter().map(|frame| frame.0));
                        self.done = true;
                        continue;
                    }
                    let mut collector = self.collector.take().expect("buffered collector");
                    for frame in tail {
                        collector.push(frame.0)?;
                    }
                    let open_tool = collector.claude_has_open_tool();
                    let body: Value = serde_json::from_slice(&collector.finish()?.into_bytes()?)
                        .map_err(|error| CoreError::Transform(error.to_string()))?;
                    let capture = Bytes::from(
                        self.raw
                            .iter()
                            .flat_map(|chunk| chunk.iter().copied())
                            .collect::<Vec<_>>(),
                    );
                    self.runner
                        .capture(http::StatusCode::OK, &self.headers, capture)
                        .await;
                    self.runner.record_wire(&body);
                    let next = if open_tool {
                        None
                    } else {
                        self.runner.next(&body).await?
                    };
                    if let Some(next) = next {
                        if !next.status().is_success() {
                            let next = crate::attempt::body::collect(next)
                                .await
                                .map_err(|error| CoreError::Transport(error.error))?;
                            self.runner
                                .capture(next.status(), next.headers(), next.body().clone())
                                .await;
                            if matches!(next.status().as_u16(), 429 | 503) {
                                let mut body = body;
                                super::buffered::recommended(
                                    &mut body,
                                    &self.runner.facts.target.upstream_model,
                                );
                                self.finish(body).await?;
                            } else {
                                self.pending.extend(gproxy_transform::synthesize_error(
                                    Kind::ClaudeMessages,
                                    StreamFraming::Sse,
                                    &super::credit::message(next.body()),
                                )?);
                                self.done = true;
                            }
                            continue;
                        }
                        let (parts, input) = next.into_parts();
                        self.input = input;
                        self.headers = parts.headers;
                        self.buffered.clear();
                        self.raw.clear();
                        self.buffered_len = 0;
                        let channel = self
                            .runner
                            .core
                            .channels
                            .get(&self.runner.facts.target.provider.channel)
                            .expect("channel");
                        self.runner.meter.start(
                            channel.stream_decoder(StreamCtx {
                                key: self.runner.facts.key.expect("Messages"),
                                framing: self.runner.facts.target_framing,
                                request_body: &self.runner.replay.body,
                                response_headers: &self.headers,
                            }),
                            self.runner.facts.target.upstream_model.clone(),
                            self.runner.facts.upstream_started_at_ms.expect("send time"),
                            self.runner.replay.body.clone(),
                        );
                        self.collector = Some(gproxy_transform::ResponseCollector::new(
                            Kind::ClaudeMessages,
                        )?);
                    } else if self.runner.sent == 1 {
                        self.runner.pin(&body).await;
                        self.pending.extend(self.buffered.drain(..));
                        self.done = true;
                    } else {
                        self.finish(body).await?;
                    }
                }
            }
        }
    }

    async fn finish(&mut self, body: Value) -> Result<(), CoreError> {
        self.runner.pin(&body).await;
        let body = Bytes::from(self.runner.outward(body).to_string());
        self.pending.extend(gproxy_transform::synthesize_response(
            Kind::ClaudeMessages,
            body,
            StreamFraming::Sse,
        )?);
        self.done = true;
        Ok(())
    }
}
