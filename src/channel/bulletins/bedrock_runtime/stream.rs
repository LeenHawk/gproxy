//! Bedrock Runtime stream shaping: AWS EventStream to Claude SSE, OpenAI SSE passthrough.

use base64::Engine;
use serde_json::{Value, json};

use crate::channel::ChannelStreamDecoder;
use crate::channel::aws_eventstream::{SmithyFrame, SmithyFrameParser, looks_like_frame};

enum WireFormat {
    Detect,
    AwsEventStream,
    Sse,
}

pub(super) struct BedrockRuntimeStreamDecoder {
    format: WireFormat,
    pending: Vec<u8>,
    parser: SmithyFrameParser,
}

impl BedrockRuntimeStreamDecoder {
    pub(super) fn new() -> Self {
        Self {
            format: WireFormat::Detect,
            pending: Vec::new(),
            parser: SmithyFrameParser::new(),
        }
    }

    fn decode_aws(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for frame in self.parser.push(chunk) {
            decode_frame(frame, &mut out);
        }
        out
    }
}

impl ChannelStreamDecoder for BedrockRuntimeStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        match self.format {
            WireFormat::Sse => chunk.to_vec(),
            WireFormat::AwsEventStream => self.decode_aws(chunk),
            WireFormat::Detect => {
                self.pending.extend_from_slice(chunk);
                match looks_like_frame(&self.pending) {
                    None => Vec::new(),
                    Some(false) => {
                        self.format = WireFormat::Sse;
                        std::mem::take(&mut self.pending)
                    }
                    Some(true) => {
                        self.format = WireFormat::AwsEventStream;
                        let bytes = std::mem::take(&mut self.pending);
                        self.decode_aws(&bytes)
                    }
                }
            }
        }
    }

    fn finish(&mut self) -> Vec<u8> {
        match self.format {
            WireFormat::Detect | WireFormat::Sse => std::mem::take(&mut self.pending),
            WireFormat::AwsEventStream if self.parser.has_pending() => error_sse(
                "truncated_eventstream",
                "Bedrock stream ended inside a frame",
            ),
            WireFormat::AwsEventStream => Vec::new(),
        }
    }
}

fn decode_frame(frame: SmithyFrame, out: &mut Vec<u8>) {
    if let Some(kind) = frame.exception_type {
        let message = frame
            .payload
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Bedrock streaming request failed");
        out.extend(error_sse(&kind, message));
        return;
    }
    if frame.event_type.as_deref() != Some("chunk") {
        return;
    }
    let Some(encoded) = frame.payload.get("bytes").and_then(Value::as_str) else {
        out.extend(error_sse(
            "invalid_eventstream_chunk",
            "Bedrock chunk did not contain bytes",
        ));
        return;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        out.extend(error_sse(
            "invalid_eventstream_chunk",
            "Bedrock chunk bytes were not valid base64",
        ));
        return;
    };
    let Ok(event) = serde_json::from_slice::<Value>(&bytes) else {
        out.extend(error_sse(
            "invalid_claude_event",
            "Bedrock chunk was not a Claude JSON event",
        ));
        return;
    };
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    out.extend_from_slice(format!("event: {event_type}\ndata: {event}\n\n").as_bytes());
}

fn error_sse(code: &str, message: &str) -> Vec<u8> {
    let event = json!({
        "type": "error",
        "error": { "type": "api_error", "code": code, "message": message }
    });
    format!("event: error\ndata: {event}\n\n").into_bytes()
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
