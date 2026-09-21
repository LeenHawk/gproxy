//! AWS event-stream frames translated into Claude Messages SSE.
//!
//! `InvokeModelWithResponseStream` does not answer with SSE. Its body is a
//! sequence of `vnd.amazon.eventstream` frames, decoded by
//! `shared::aws_eventstream`, which `kiro` reads too; what a decoded payload
//! means is what is left here (v3 `aws_bedrock/sse/invoke.rs`).
//!
//! For an Anthropic model every `:event-type: chunk` frame carries a JSON
//! payload whose `bytes` field is base64 of one Claude Messages SSE event
//! object. Re-emitting those as `event: {type}\ndata: {json}\n\n` gives the
//! client exactly the Messages stream it asked for, so this is the only place
//! the channel rewrites a delivered body. A frame whose `:message-type` is
//! `exception` becomes a Messages `error` event and closes the stream.

use crate::channel::ChannelError;
use crate::channels::shared::aws_eventstream::{FrameParser, decode};
use base64::Engine as _;
use gproxy_protocol::connection::{Bytes, TransportError};
use serde_json::{Value, json};

/// Event-stream frames in, Claude Messages SSE bytes out.
#[derive(Default)]
pub(super) struct Translator {
    parser: FrameParser,
    stopped: bool,
    failed: bool,
}

impl Translator {
    pub(super) fn new() -> Self {
        Self {
            parser: FrameParser::new(),
            stopped: false,
            failed: false,
        }
    }

    /// True once an exception frame ended the message; the caller stops
    /// reading the upstream body.
    pub(super) fn failed(&self) -> bool {
        self.failed
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Bytes, ChannelError> {
        let mut out = Vec::new();
        for frame in self.parser.push(chunk)? {
            if frame.exception_type.is_some() || frame.message_type.as_deref() == Some("exception")
            {
                self.failed = true;
                let kind = frame
                    .exception_type
                    .as_deref()
                    .unwrap_or("stream_exception")
                    .to_owned();
                let message = serde_json::from_slice::<Value>(&frame.payload)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("message")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| kind.clone());
                out.extend_from_slice(&encode(&json!({
                    "type": "error",
                    "error": {"type": kind, "message": message}
                })));
                break;
            }
            if frame.event_type.as_deref() != Some("chunk") {
                continue;
            }
            let encoded = serde_json::from_slice::<Value>(&frame.payload)
                .ok()
                .as_ref()
                .and_then(|payload| payload.get("bytes"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| decode("a chunk frame carries no base64 `bytes`"))?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| decode(format!("chunk base64: {error}")))?;
            let event: Value = serde_json::from_slice(&decoded)
                .map_err(|error| decode(format!("chunk payload JSON: {error}")))?;
            self.stopped |= event.get("type").and_then(Value::as_str) == Some("message_stop");
            out.extend_from_slice(&encode(&event));
        }
        Ok(Bytes::from(out))
    }

    /// Upstream EOF. A clean end must have delivered `message_stop`.
    pub(super) fn finish(&mut self) -> Result<(), ChannelError> {
        if self.failed {
            return Ok(());
        }
        self.parser.finish()?;
        if self.stopped {
            Ok(())
        } else {
            Err(decode("the stream ended before message_stop"))
        }
    }
}

/// `event: {type}\ndata: {json}\n\n`, the Claude Messages SSE framing.
pub(super) fn encode(event: &Value) -> Bytes {
    let name = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    Bytes::from(format!("event: {name}\ndata: {event}\n\n"))
}

pub(super) fn transport_error(error: ChannelError) -> TransportError {
    error.to_string().into()
}
