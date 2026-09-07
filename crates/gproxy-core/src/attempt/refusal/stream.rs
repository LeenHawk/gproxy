use std::collections::VecDeque;

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{StreamCtx, StreamEnd, TransportError};
use gproxy_protocol::{ContentGenerationKind as Kind, StreamFraming};
use serde_json::Value;

use crate::boundary::ByteStream;
use crate::error::CoreError;
use crate::host::Host;

use super::{retry::Runner, stream_events::Events};

struct State<H: Host> {
    runner: Runner<H>,
    input: ByteStream,
    headers: http::HeaderMap,
    events: Events,
    raw: Vec<Bytes>,
    raw_len: usize,
    pending: VecDeque<Bytes>,
    completion: Option<(Value, bool)>,
    done: bool,
}

pub(super) fn wrap<H: Host>(
    runner: Runner<H>,
    response: http::Response<ByteStream>,
) -> http::Response<ByteStream> {
    let (mut parts, input) = response.into_parts();
    let mut state = State {
        runner,
        input,
        headers: parts.headers.clone(),
        events: Events::new(),
        raw: Vec::new(),
        raw_len: 0,
        pending: VecDeque::new(),
        completion: None,
        done: false,
    };
    state.start();
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
    fn start(&mut self) {
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
    }

    async fn next(&mut self) -> Result<Option<Bytes>, CoreError> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Ok(Some(frame));
            }
            if self.done {
                return Ok(None);
            }
            if let Some((body, open_tool)) = self.completion.take() {
                self.complete(body, open_tool).await?;
                continue;
            }
            match self.input.next().await {
                Some(Ok(chunk)) => {
                    self.raw_len = self.raw_len.saturating_add(chunk.len());
                    if self.raw_len > 100 * 1024 * 1024 {
                        return Err(CoreError::Transform(
                            "fallback response history exceeds 100 MiB".into(),
                        ));
                    }
                    self.raw.push(chunk.clone());
                    for frame in self.runner.meter.push(chunk)? {
                        self.pending.extend(self.events.push(frame.0)?);
                    }
                }
                Some(Err(error)) => {
                    self.runner.meter.finish(StreamEnd::Interrupted, false)?;
                    return Err(error.into());
                }
                None => {
                    for frame in self.runner.meter.finish(StreamEnd::Complete, false)? {
                        self.pending.extend(self.events.push(frame.0)?);
                    }
                    let (frames, body, open_tool) = self.events.end()?;
                    self.pending.extend(frames);
                    self.completion = Some((body, open_tool));
                }
            }
        }
    }

    async fn complete(&mut self, mut body: Value, open_tool: bool) -> Result<(), CoreError> {
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
        let sent = self.runner.sent;
        let next = if open_tool {
            None
        } else {
            self.runner
                .next_streaming(&body, self.events.output)
                .await?
        };
        let Some(next) = next else {
            if self.runner.sent > sent {
                super::buffered::recommended(&mut body, &self.runner.facts.target.upstream_model);
            }
            self.finish(body).await;
            return Ok(());
        };
        if !next.status().is_success() {
            let next = crate::attempt::body::collect(next)
                .await
                .map_err(|error| CoreError::Transport(error.error))?;
            self.runner
                .capture(next.status(), next.headers(), next.body().clone())
                .await;
            if matches!(next.status().as_u16(), 429 | 503) {
                super::buffered::recommended(&mut body, &self.runner.facts.target.upstream_model);
                self.finish(body).await;
            } else {
                self.pending.extend(gproxy_transform::synthesize_error(
                    Kind::ClaudeMessages,
                    StreamFraming::Sse,
                    &super::credit::message(next.body()),
                )?);
                self.done = true;
            }
            return Ok(());
        }
        let (parts, input) = next.into_parts();
        self.input = input;
        self.headers = parts.headers;
        self.raw.clear();
        self.raw_len = 0;
        self.events.retry(
            self.runner
                .boundaries
                .last()
                .expect("accepted fallback boundary")
                .clone(),
        );
        self.start();
        Ok(())
    }

    async fn finish(&mut self, body: Value) {
        self.runner.pin(&body).await;
        self.pending
            .extend(self.events.finish(&body, &self.runner.wire_iterations));
        self.done = true;
    }
}
