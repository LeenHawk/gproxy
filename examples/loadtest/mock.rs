//! Mock upstream: canned per-wire responses keyed by request path.
//!
//! No request capture (no `Mutex<Vec<..>>`) — the hot path must stay free of
//! lock/allocation noise. SSE chunk vectors are prebuilt once and shared via
//! `Arc`; per-request cost is a refcount clone per frame.

use std::sync::Arc;

use bytes::Bytes;
use futures_util::StreamExt as _;
use gproxy::http::client::{ClientError, RespStream, UpstreamClient};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

/// One content frame worth of text (~20 chars).
pub const FRAME_TEXT: &str = "0123456789abcdefghij";
const MODEL: &str = "up-model";

pub struct MockUpstream {
    chat_full: Bytes,
    resp_full: Bytes,
    cla_full: Bytes,
    gem_full: Bytes,
    chat_sse: Arc<Vec<Bytes>>,
    resp_sse: Arc<Vec<Bytes>>,
    cla_sse: Arc<Vec<Bytes>>,
    gem_sse: Arc<Vec<Bytes>>,
}

impl MockUpstream {
    /// Build all fixtures once; `events` = number of content delta frames.
    pub fn new(events: usize) -> Self {
        let full = FRAME_TEXT.repeat(events);
        Self {
            chat_full: to_bytes(&chat_full(&full, events)),
            resp_full: to_bytes(&responses_object("completed", &full, Some(events))),
            cla_full: to_bytes(&claude_full(&full, events)),
            gem_full: to_bytes(&gemini_tail(&full, events)),
            chat_sse: Arc::new(chat_sse(events)),
            resp_sse: Arc::new(responses_sse(events)),
            cla_sse: Arc::new(claude_sse(events)),
            gem_sse: Arc::new(gemini_sse(events)),
        }
    }

    fn full_for(&self, uri: &str) -> Option<Bytes> {
        if uri.contains("/chat/completions") {
            Some(self.chat_full.clone())
        } else if uri.contains("/responses") {
            Some(self.resp_full.clone())
        } else if uri.contains("/messages") {
            Some(self.cla_full.clone())
        } else if uri.contains(":generateContent") {
            Some(self.gem_full.clone())
        } else {
            None
        }
    }

    fn sse_for(&self, uri: &str) -> Option<&Arc<Vec<Bytes>>> {
        if uri.contains("/chat/completions") {
            Some(&self.chat_sse)
        } else if uri.contains("/responses") {
            Some(&self.resp_sse)
        } else if uri.contains("/messages") {
            Some(&self.cla_sse)
        } else if uri.contains(":streamGenerateContent") {
            Some(&self.gem_sse)
        } else {
            None
        }
    }
}

#[async_trait::async_trait]
impl UpstreamClient for MockUpstream {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        let uri = req.uri().to_string();
        let body = self
            .full_for(&uri)
            .ok_or_else(|| ClientError::Config(format!("mock: unknown non-stream path {uri}")))?;
        http::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(body)
            .map_err(|e| ClientError::Transport(e.to_string()))
    }

    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<(StatusCode, HeaderMap, RespStream), ClientError> {
        let uri = req.uri().to_string();
        let chunks = Arc::clone(
            self.sse_for(&uri)
                .ok_or_else(|| ClientError::Config(format!("mock: unknown stream path {uri}")))?,
        );
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "text/event-stream".parse().unwrap());
        let len = chunks.len();
        let stream = futures_util::stream::iter(0..len)
            .map(move |i| Ok::<Bytes, ClientError>(chunks[i].clone()));
        Ok((StatusCode::OK, headers, Box::pin(stream)))
    }
    // `open_websocket` keeps the default Err impl — websockets are out of scope.
}

fn to_bytes(v: &Value) -> Bytes {
    Bytes::from(serde_json::to_vec(v).expect("fixture json"))
}

fn data(v: &Value) -> Bytes {
    Bytes::from(format!("data: {v}\n\n"))
}

fn event(name: &str, v: &Value) -> Bytes {
    Bytes::from(format!("event: {name}\ndata: {v}\n\n"))
}

/// SSE frame whose data JSON carries the `"type"` discriminator (OpenAI
/// responses / Claude wire shape).
fn typed_event(name: &str, v: &Value) -> Bytes {
    let mut v = v.clone();
    v["type"] = json!(name);
    event(name, &v)
}

fn usage_openai(events: usize) -> Value {
    json!({ "prompt_tokens": 10, "completion_tokens": events, "total_tokens": 10 + events })
}

// ── OpenAI chat completions wire ─────────────────────────────────────────────

fn chat_full(text: &str, events: usize) -> Value {
    json!({
        "id": "chatcmpl-lt", "object": "chat.completion", "created": 1, "model": MODEL,
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": text },
                      "finish_reason": "stop" }],
        "usage": usage_openai(events)
    })
}

fn chat_chunk(delta: Value, finish: Option<&str>) -> Value {
    json!({
        "id": "chatcmpl-lt", "object": "chat.completion.chunk", "created": 1, "model": MODEL,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }]
    })
}

fn chat_sse(events: usize) -> Vec<Bytes> {
    let mut v = Vec::with_capacity(events + 4);
    v.push(data(&chat_chunk(
        json!({ "role": "assistant", "content": "" }),
        None,
    )));
    for _ in 0..events {
        v.push(data(&chat_chunk(json!({ "content": FRAME_TEXT }), None)));
    }
    v.push(data(&chat_chunk(json!({}), Some("stop"))));
    // stream_options.include_usage tail chunk: empty choices + usage.
    v.push(data(&json!({
        "id": "chatcmpl-lt", "object": "chat.completion.chunk", "created": 1, "model": MODEL,
        "choices": [], "usage": usage_openai(events)
    })));
    v.push(Bytes::from_static(b"data: [DONE]\n\n"));
    v
}

// ── OpenAI responses wire ────────────────────────────────────────────────────

fn responses_message_item(text: &str, status: &str) -> Value {
    let content = if text.is_empty() {
        json!([])
    } else {
        json!([{ "type": "output_text", "text": text, "annotations": [] }])
    };
    json!({ "type": "message", "id": "msg_1", "role": "assistant",
            "content": content, "status": status })
}

fn responses_object(status: &str, text: &str, usage_events: Option<usize>) -> Value {
    let output = if text.is_empty() {
        json!([])
    } else {
        json!([responses_message_item(text, "completed")])
    };
    let mut obj = json!({
        "id": "resp_1", "object": "response", "created_at": 1, "model": MODEL,
        "status": status, "output": output
    });
    if let Some(events) = usage_events {
        obj["usage"] = json!({
            "input_tokens": 10, "output_tokens": events, "total_tokens": 10 + events,
            "output_tokens_details": { "reasoning_tokens": 0 }
        });
    }
    obj
}

fn responses_sse(events: usize) -> Vec<Bytes> {
    let full = FRAME_TEXT.repeat(events);
    let part = |text: &str| json!({ "type": "output_text", "text": text, "annotations": [] });
    let mut v = Vec::with_capacity(events + 7);
    v.push(typed_event(
        "response.created",
        &json!({ "response": responses_object("in_progress", "", None) }),
    ));
    v.push(typed_event(
        "response.output_item.added",
        &json!({ "item": responses_message_item("", "in_progress"), "output_index": 0 }),
    ));
    v.push(typed_event(
        "response.content_part.added",
        &json!({ "content_index": 0, "item_id": "msg_1", "output_index": 0, "part": part("") }),
    ));
    for _ in 0..events {
        v.push(typed_event(
            "response.output_text.delta",
            &json!({ "content_index": 0, "delta": FRAME_TEXT, "item_id": "msg_1",
                     "output_index": 0 }),
        ));
    }
    v.push(typed_event(
        "response.output_text.done",
        &json!({ "content_index": 0, "item_id": "msg_1", "output_index": 0, "text": full }),
    ));
    v.push(typed_event(
        "response.content_part.done",
        &json!({ "content_index": 0, "item_id": "msg_1", "output_index": 0, "part": part(&full) }),
    ));
    v.push(typed_event(
        "response.output_item.done",
        &json!({ "item": responses_message_item(&full, "completed"), "output_index": 0 }),
    ));
    v.push(typed_event(
        "response.completed",
        &json!({ "response": responses_object("completed", &full, Some(events)) }),
    ));
    v
}

// ── Claude messages wire ─────────────────────────────────────────────────────

fn claude_full(text: &str, events: usize) -> Value {
    json!({
        "id": "msg_1", "type": "message", "role": "assistant", "model": MODEL,
        "content": [{ "type": "text", "text": text }],
        "stop_reason": "end_turn", "stop_sequence": null,
        "usage": { "input_tokens": 10, "output_tokens": events }
    })
}

fn claude_sse(events: usize) -> Vec<Bytes> {
    let mut v = Vec::with_capacity(events + 5);
    v.push(event(
        "message_start",
        &json!({ "type": "message_start", "message": {
            "id": "msg_1", "type": "message", "role": "assistant", "content": [],
            "model": MODEL, "stop_reason": null, "stop_sequence": null,
            "usage": { "input_tokens": 10, "output_tokens": 1 }
        }}),
    ));
    v.push(event(
        "content_block_start",
        &json!({ "type": "content_block_start", "index": 0,
                 "content_block": { "type": "text", "text": "" } }),
    ));
    for _ in 0..events {
        v.push(event(
            "content_block_delta",
            &json!({ "type": "content_block_delta", "index": 0,
                     "delta": { "type": "text_delta", "text": FRAME_TEXT } }),
        ));
    }
    v.push(event(
        "content_block_stop",
        &json!({ "type": "content_block_stop", "index": 0 }),
    ));
    v.push(event(
        "message_delta",
        &json!({ "type": "message_delta",
                 "delta": { "stop_reason": "end_turn", "stop_sequence": null },
                 "usage": { "input_tokens": 10, "output_tokens": events } }),
    ));
    v.push(event("message_stop", &json!({ "type": "message_stop" })));
    v
}

// ── Gemini generateContent wire ──────────────────────────────────────────────

fn gemini_chunk(text: &str) -> Value {
    json!({
        "candidates": [{ "content": { "role": "model", "parts": [{ "text": text }] },
                         "index": 0 }],
        "modelVersion": MODEL
    })
}

fn gemini_tail(text: &str, events: usize) -> Value {
    json!({
        "candidates": [{ "content": { "role": "model", "parts": [{ "text": text }] },
                         "finishReason": "STOP", "index": 0 }],
        "usageMetadata": { "promptTokenCount": 10, "candidatesTokenCount": events,
                           "totalTokenCount": 10 + events },
        "modelVersion": MODEL
    })
}

fn gemini_sse(events: usize) -> Vec<Bytes> {
    let mut v = Vec::with_capacity(events + 1);
    for _ in 0..events {
        v.push(data(&gemini_chunk(FRAME_TEXT)));
    }
    v.push(data(&gemini_tail("", events)));
    v
}
