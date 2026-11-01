//! Reading enough of an upstream answer to tell what a refusal means for the
//! credential: a bounded prefix of a non-2xx body, and the first meaningful
//! frame of a 2xx event stream, which can still be an error. Whatever is read
//! is replayed ahead of the rest of the body, so the caller receives the
//! answer unchanged and nothing past the prefix is copied.

use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody,
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::{ByteStream, Bytes, TransportError},
};
use http::StatusCode;
use serde_json::Value;
use std::time::Duration;
use web_time::Instant;

/// How much of a refusal's body is read to classify it. Error bodies are
/// small; a larger one is classified from its beginning.
const BODY_PREFIX_LIMIT: usize = 64 * 1024;
/// A refusal body that trickles is classified from what arrived by then.
const BODY_PREFIX_WAIT: Duration = Duration::from_secs(5);
/// How long a 2xx stream may stay in its preamble (pings, `message_start`,
/// `response.created`, a role-only chat chunk) before it is handed to the
/// caller unexamined.
const FIRST_FRAME_WAIT: Duration = Duration::from_secs(10);
const FIRST_FRAME_BYTES: usize = 64 * 1024;

/// An error frame that opened a 2xx stream: the status its error stands for
/// and the error as JSON, for `BaseChannel::classify_rejection`.
pub(super) struct StreamRefusal {
    pub status: StatusCode,
    pub payload: Bytes,
}

fn take(body: &mut HttpBody) -> Option<futures_util::stream::Fuse<ByteStream>> {
    match body {
        HttpBody::Stream(stream) => {
            let empty: ByteStream = Box::pin(futures_util::stream::empty());
            Some(std::mem::replace(stream, empty).fuse())
        }
        HttpBody::Bytes(_) => None,
    }
}

/// Put what was read back in front of the rest of the stream.
fn replay(
    body: &mut HttpBody,
    read: Vec<Bytes>,
    failed: Option<TransportError>,
    rest: futures_util::stream::Fuse<ByteStream>,
) {
    let head = futures_util::stream::iter(read.into_iter().map(Ok).chain(failed.map(Err)));
    *body = HttpBody::Stream(Box::pin(head.chain(rest)));
}

/// The first `BODY_PREFIX_LIMIT` bytes of a refusal's body.
pub(super) async fn body_prefix(body: &mut HttpBody) -> Bytes {
    let Some(mut stream) = take(body) else {
        let HttpBody::Bytes(bytes) = body else {
            unreachable!("not a stream")
        };
        return bytes.slice(..bytes.len().min(BODY_PREFIX_LIMIT));
    };
    let deadline = Instant::now() + BODY_PREFIX_WAIT;
    let mut read = Vec::new();
    let mut length = 0;
    let mut failed = None;
    while length < BODY_PREFIX_LIMIT {
        let left = deadline.saturating_duration_since(Instant::now());
        match crate::rt::timeout(left, stream.next()).await {
            Some(Some(Ok(chunk))) => {
                length += chunk.len();
                read.push(chunk);
            }
            Some(Some(Err(error))) => {
                failed = Some(error);
                break;
            }
            Some(None) | None => break,
        }
    }
    let prefix = match read.as_slice() {
        [one] => one.slice(..one.len().min(BODY_PREFIX_LIMIT)),
        chunks => {
            let mut joined = Vec::with_capacity(length.min(BODY_PREFIX_LIMIT));
            for chunk in chunks {
                let room = BODY_PREFIX_LIMIT - joined.len();
                joined.extend_from_slice(&chunk[..chunk.len().min(room)]);
            }
            Bytes::from(joined)
        }
    };
    replay(body, read, failed, stream);
    prefix
}

enum Verdict {
    Content,
    Error(StreamRefusal),
}

/// Read a 2xx event stream up to its first meaningful frame. Returns the
/// refusal when that frame is an error; the body is replayed either way.
pub(super) async fn first_frame_error(
    body: &mut HttpBody,
    limits: CodecLimits,
) -> Option<StreamRefusal> {
    let mut stream = take(body)?;
    let mut decoder = SseDecoder::new(limits);
    let deadline = Instant::now() + FIRST_FRAME_WAIT;
    let mut read = Vec::new();
    let mut length = 0;
    let mut failed = None;
    let verdict = loop {
        if length >= FIRST_FRAME_BYTES {
            break None;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match crate::rt::timeout(left, stream.next()).await {
            Some(None) | None => break None,
            Some(Some(Err(error))) => {
                // A converted stream fails with the upstream's error before
                // writing any client frame for it.
                let found =
                    crate::convert::stream_error::detail_of(&error).map(|detail| StreamRefusal {
                        status: status_of(&detail),
                        payload: Bytes::from(
                            serde_json::to_vec(&serde_json::json!({ "error": detail }))
                                .unwrap_or_default(),
                        ),
                    });
                failed = Some(error);
                break found;
            }
            Some(Some(Ok(chunk))) => {
                length += chunk.len();
                let frames = decoder.push(&chunk);
                read.push(chunk);
                let Ok(frames) = frames else {
                    break None;
                };
                match frames.iter().find_map(verdict) {
                    Some(Verdict::Content) => break None,
                    Some(Verdict::Error(refusal)) => break Some(refusal),
                    None => {}
                }
            }
        }
    };
    replay(body, read, failed, stream);
    verdict
}

fn status_of(error: &Value) -> StatusCode {
    StatusCode::from_u16(crate::convert::upstream_error::status(error))
        .unwrap_or(StatusCode::BAD_GATEWAY)
}

/// None for preamble frames that carry nothing yet.
fn verdict(frame: &SseFrame) -> Option<Verdict> {
    let SseFrame::Event(event) = frame else {
        return Some(Verdict::Content);
    };
    if event.event.as_deref() == Some("ping") || event.data.trim().is_empty() {
        return None;
    }
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return Some(Verdict::Content);
    };
    if event.event.as_deref() == Some("error")
        || crate::convert::stream_error::is_error_frame(frame)
    {
        let error = gproxy_channel::channel::error_object(&value).unwrap_or(&value);
        return Some(Verdict::Error(StreamRefusal {
            status: status_of(error),
            payload: Bytes::from(event.data.clone()),
        }));
    }
    if matches!(
        value.get("type").and_then(Value::as_str),
        Some(
            "ping"
                | "message_start"
                | "response.created"
                | "response.in_progress"
                | "response.queued"
        )
    ) {
        return None;
    }
    // A chat chunk naming only the role, before any content.
    if let Some(choices) = value.get("choices").and_then(Value::as_array)
        && value.get("usage").is_none_or(Value::is_null)
        && choices.iter().all(|choice| {
            choice.get("finish_reason").is_none_or(Value::is_null)
                && choice
                    .get("delta")
                    .and_then(Value::as_object)
                    .is_none_or(|delta| {
                        delta.iter().all(|(key, value)| {
                            key == "role"
                                || value.is_null()
                                || value.as_str().is_some_and(str::is_empty)
                        })
                    })
        })
    {
        return None;
    }
    Some(Verdict::Content)
}
