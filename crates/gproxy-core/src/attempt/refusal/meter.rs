use std::sync::{Arc, Mutex};

use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, Frame, NormalizedUsage, StreamDecoder, StreamEnd, StreamTail, UsageAttempt,
};

#[derive(Clone)]
pub(super) struct Meter(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    decoder: Option<Box<dyn StreamDecoder>>,
    attempts: Vec<UsageAttempt>,
    latest: Option<NormalizedUsage>,
    model: String,
    started: Option<i64>,
    input: Bytes,
    received: u64,
}

impl Meter {
    pub(super) fn new() -> Self {
        Self(Arc::new(Mutex::new(State::default())))
    }

    pub(super) fn start(
        &self,
        decoder: Option<Box<dyn StreamDecoder>>,
        model: String,
        started: i64,
        input: Bytes,
    ) {
        let mut state = self.0.lock().expect("fallback meter lock");
        state.decoder = decoder;
        state.model = model;
        state.started = Some(started);
        state.input = input;
        state.received = 0;
    }

    pub(super) fn push(&self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        let mut state = self.0.lock().expect("fallback meter lock");
        state.received = state
            .received
            .saturating_add(crate::usage::utf8_chars(&chunk));
        match state.decoder.as_mut() {
            Some(decoder) => decoder.push(chunk),
            None => Ok(vec![Frame(chunk)]),
        }
    }

    pub(super) fn finish(&self, end: StreamEnd, refused: bool) -> Result<Vec<Frame>, ChannelError> {
        let mut state = self.0.lock().expect("fallback meter lock");
        let Some(mut decoder) = state.decoder.take() else {
            return Ok(Vec::new());
        };
        let tail = decoder.finish(end)?;
        let estimated = tail.usage.is_none();
        let usage = tail.usage.unwrap_or_else(|| NormalizedUsage {
            input_tokens: crate::usage::estimate_input_tokens(&state.input),
            output_tokens: state.received.div_ceil(2),
            ..Default::default()
        });
        state.record(usage, refused, estimated);
        Ok(tail.frames)
    }

    pub(super) fn record(
        &self,
        usage: NormalizedUsage,
        model: String,
        started: i64,
        refused: bool,
        estimated: bool,
    ) {
        let mut state = self.0.lock().expect("fallback meter lock");
        state.model = model;
        state.started = Some(started);
        state.record(usage, refused, estimated);
    }

    pub(super) fn usage(&self) -> Option<NormalizedUsage> {
        let state = self.0.lock().expect("fallback meter lock");
        state.latest.clone().map(|mut usage| {
            usage.attempts = state.attempts.clone();
            usage
        })
    }

    pub(super) fn len(&self) -> usize {
        self.0.lock().expect("meter").attempts.len()
    }

    pub(super) fn reject(&self, model: String, started: i64, status: u16) {
        let mut usage = NormalizedUsage::default();
        usage
            .dimensions
            .insert("http_status".into(), status.to_string());
        self.record(usage, model, started, true, false);
    }

    pub(super) fn decoder(&self) -> Box<dyn StreamDecoder> {
        Box::new(Observer(self.clone()))
    }
}

impl State {
    fn record(&mut self, mut usage: NormalizedUsage, refused: bool, estimated: bool) {
        let mut attempts = std::mem::take(&mut usage.attempts);
        if attempts.is_empty() {
            attempts.push(UsageAttempt {
                model: self.model.clone(),
                usage: Box::new(usage.clone()),
                billable: !refused || usage.output_tokens > 0,
                estimated,
                started_at_ms: self.started,
            });
        }
        for attempt in &mut attempts {
            if attempt.model.is_empty() {
                attempt.model.clone_from(&self.model);
            }
            attempt.started_at_ms = attempt.started_at_ms.or(self.started);
        }
        self.attempts.extend(attempts);
        self.latest = Some(usage);
    }
}

struct Observer(Meter);

impl StreamDecoder for Observer {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        Ok(vec![Frame(chunk)])
    }
    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        self.0.finish(end, false)?;
        Ok(StreamTail {
            usage: self.0.usage(),
            ..Default::default()
        })
    }
}
