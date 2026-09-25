//! Prefix-only buffering and credit-aware continuation. Content is yielded as
//! it arrives; only a terminal refusal is withheld while deciding whether a
//! credit permits continuing it. No tokenless restart after output.
use super::*;
use futures_util::StreamExt;
use gproxy_protocol::{
    codec::{CodecLimits, SseDecoder, SseEvent, SseFrame, encode_sse_done, encode_sse_event},
    connection::{ByteStream, TransportError},
    transform::generate::stream::claude::{ClaudeStreamCollector, ClaudeStreamLimits},
};
use std::collections::VecDeque;

const PREFIX_LIMIT: usize = 16 * 1024;

fn stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::iter([Ok(bytes)])),
        HttpBody::Stream(body) => body,
    }
}
fn error(message: impl Into<String>) -> TransportError {
    Box::new(std::io::Error::other(message.into()))
}
fn encode(value: &Value, limits: CodecLimits) -> Result<Bytes, TransportError> {
    encode_sse_event(
        &SseEvent {
            event: value["type"].as_str().map(str::to_owned),
            id: None,
            retry: None,
            data: value.to_string(),
        },
        limits,
    )
    .map_err(|e| error(e.to_string()))
}

struct Events {
    decoder: SseDecoder,
    collector: Option<ClaudeStreamCollector>,
    limits: CodecLimits,
    prefix: VecDeque<Bytes>,
    prefix_bytes: usize,
    terminal: VecDeque<Bytes>,
    started: bool,
    emitted: bool,
    offset: u64,
    next_index: u64,
    boundary: Option<Value>,
    start_usage: serde_json::Map<String, Value>,
}
impl Events {
    fn new(limits: CodecLimits) -> Self {
        Self {
            decoder: SseDecoder::new(limits),
            collector: Some(Self::collector(limits)),
            limits,
            prefix: VecDeque::new(),
            prefix_bytes: 0,
            terminal: VecDeque::new(),
            started: false,
            emitted: false,
            offset: 0,
            next_index: 0,
            boundary: None,
            start_usage: Default::default(),
        }
    }
    fn collector(limits: CodecLimits) -> ClaudeStreamCollector {
        ClaudeStreamCollector::new(ClaudeStreamLimits {
            max_events: 100_000,
            max_blocks: 10_000,
            max_text_bytes: limits.max_body_bytes.min(usize::MAX as u64) as usize,
            max_json_bytes: limits.max_body_bytes.min(usize::MAX as u64) as usize,
        })
    }
    fn retry(&mut self, from: &Value, model: &str) {
        self.decoder = SseDecoder::new(self.limits);
        self.collector = Some(Self::collector(self.limits));
        self.prefix.clear();
        self.prefix_bytes = 0;
        self.terminal.clear();
        self.start_usage.clear();
        if self.started {
            self.boundary = Some(
                json!({"type":"fallback","trigger":{"type":"refusal"},"from":{"model":from},"to":{"model":model}}),
            );
        } else {
            self.next_index = 0;
            self.offset = 0;
        }
    }
    fn frame(&mut self, frame: SseFrame, out: &mut VecDeque<Bytes>) -> Result<(), TransportError> {
        let SseFrame::Event(mut event) = frame else {
            self.terminal.push_back(encode_sse_done());
            return Ok(());
        };
        let mut value: Value =
            serde_json::from_str(&event.data).map_err(|e| error(e.to_string()))?;
        // A response we cannot faithfully collect is still forwarded, but
        // cannot authorize a replay.
        if let Some(collector) = self.collector.as_mut() {
            if serde_json::from_value(value.clone())
                .ok()
                .is_none_or(|typed| collector.push(typed).is_err())
            {
                self.collector = None;
            }
        }
        let kind = value["type"].as_str().unwrap_or_default().to_owned();
        if kind == "message_start" {
            self.start_usage = value
                .pointer("/message/usage")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
        }
        if kind == "message_delta"
            && let Some(usage) = value.get_mut("usage").and_then(Value::as_object_mut)
        {
            // Later message_start events are hidden during continuation, so
            // repeat their input/cache counts on the visible final delta.
            for (key, count) in &self.start_usage {
                usage.entry(key.clone()).or_insert_with(|| count.clone());
            }
            event.data = value.to_string();
        }
        if (kind == "message_delta" && !value["delta"]["stop_reason"].is_null())
            || kind == "message_stop"
        {
            self.terminal.push_back(
                encode_sse_event(&event, self.limits).map_err(|e| error(e.to_string()))?,
            );
            return Ok(());
        }
        if kind == "message_start" && self.started {
            if let Some(mut boundary) = self.boundary.take() {
                if let Some(model) = value.pointer("/message/model") {
                    boundary["to"]["model"] = model.clone();
                }
                let index = self.next_index;
                self.next_index += 1;
                self.offset = self.next_index;
                out.push_back(encode(
                    &json!({"type":"content_block_start","index":index,"content_block":boundary}),
                    self.limits,
                )?);
                out.push_back(encode(
                    &json!({"type":"content_block_stop","index":index}),
                    self.limits,
                )?);
            }
            return Ok(());
        }
        if let Some(index) = value.get("index").and_then(Value::as_u64) {
            let index = index
                .checked_add(self.offset)
                .ok_or_else(|| error("fallback block index overflow"))?;
            self.next_index = self.next_index.max(index.saturating_add(1));
            if self.offset != 0 {
                value["index"] = json!(index);
                event.data = value.to_string();
            }
        }
        let content = match kind.as_str() {
            "content_block_delta" => value.get("delta").is_some_and(|delta| {
                ["text", "thinking", "partial_json", "signature"]
                    .iter()
                    .any(|key| {
                        delta
                            .get(*key)
                            .and_then(Value::as_str)
                            .is_some_and(|s| !s.is_empty())
                    })
            }),
            "content_block_start" => value.get("content_block").is_some_and(|block| {
                !matches!(block["type"].as_str(), Some("text" | "thinking"))
                    || ["text", "thinking"].iter().any(|key| {
                        block
                            .get(*key)
                            .and_then(Value::as_str)
                            .is_some_and(|s| !s.is_empty())
                    })
            }),
            _ => false,
        };
        self.emitted |= content;
        let bytes = encode_sse_event(&event, self.limits).map_err(|e| error(e.to_string()))?;
        if self.started {
            out.push_back(bytes);
        } else {
            self.prefix_bytes += bytes.len();
            self.prefix.push_back(bytes);
            if content
                || matches!(kind.as_str(), "ping" | "error")
                || self.prefix_bytes >= PREFIX_LIMIT
            {
                self.started = true;
                out.append(&mut self.prefix);
            }
        }
        Ok(())
    }
    fn finish(&mut self, out: &mut VecDeque<Bytes>) {
        out.append(&mut self.prefix);
        out.append(&mut self.terminal);
    }
}
struct State {
    call: NativeCall,
    template: WireRequest<Bytes>,
    policy: Policy,
    body: Option<ByteStream>,
    events: Events,
    pending: VecDeque<Bytes>,
    ended: bool,
}

pub(super) fn wrap(
    call: NativeCall,
    template: WireRequest<Bytes>,
    policy: Policy,
    mut response: WireResponse<HttpBody>,
) -> WireResponse<HttpBody> {
    let events = Events::new(call.attempt.request.snapshot.limits.codec());
    response.headers.remove(http::header::CONTENT_LENGTH);
    let state = State {
        call,
        template,
        policy,
        body: Some(stream(response.body)),
        events,
        pending: VecDeque::new(),
        ended: false,
    };
    response.body = HttpBody::Stream(Box::pin(futures_util::stream::unfold(
        state,
        |mut state| async move {
            loop {
                if let Some(bytes) = state.pending.pop_front() {
                    return Some((Ok(bytes), state));
                }
                if state.ended {
                    return None;
                }
                match advance(&mut state).await {
                    Ok(()) => {}
                    Err(error) => {
                        state.ended = true;
                        state.body = None;
                        return Some((Err(error), state));
                    }
                }
            }
        },
    )));
    response
}

async fn advance(state: &mut State) -> Result<(), TransportError> {
    let request = &state.call.attempt.request;
    let remaining = request
        .deadline
        .map(|deadline| deadline.saturating_duration_since(web_time::Instant::now()));
    let timeout = remaining.map_or(state.call.limits.stream_idle, |left| {
        left.min(state.call.limits.stream_idle)
    });
    let next = tokio::select! {
        biased;
        () = request.cancellation.cancelled() => return Err(error("request cancelled")),
        next = crate::rt::timeout(timeout, state.body.as_mut().expect("active stream").next()) =>
            next.ok_or_else(|| error("fallback stream deadline exceeded"))?,
    };
    if let Some(chunk) = next {
        for frame in state
            .events
            .decoder
            .push(&chunk?)
            .map_err(|e| error(e.to_string()))?
        {
            state.events.frame(frame, &mut state.pending)?;
        }
        return Ok(());
    }
    state.body = None;
    for frame in state
        .events
        .decoder
        .finish()
        .map_err(|e| error(e.to_string()))?
    {
        state.events.frame(frame, &mut state.pending)?;
    }
    let refused = state
        .events
        .collector
        .take()
        .and_then(|c| c.finish().ok())
        .and_then(|collected| serde_json::to_value(collected.value).ok());
    if let Some(refused) = refused
        && let Some(mut plan) = state
            .policy
            .plan(&state.call, &refused, state.events.emitted)
    {
        let response = send_plan(
            &state.call,
            &state.template,
            &mut state.policy,
            &mut plan,
            state.events.emitted,
        )
        .await
        .map_err(|e| error(e.to_string()))?;
        if response.status.is_success() {
            state.events.retry(&refused["model"], &plan.model);
            state.body = Some(stream(response.body));
            return Ok(());
        }
        // The downstream already has a successful HTTP head. Drain the failed
        // retry for accounting, then finish the original refusal faithfully.
        collect(&state.call, response.body)
            .await
            .map_err(|e| error(e.to_string()))?;
    }
    state.events.finish(&mut state.pending);
    state.ended = true;
    Ok(())
}
