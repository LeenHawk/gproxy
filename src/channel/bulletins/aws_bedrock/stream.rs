use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::channel::ChannelStreamDecoder;
use crate::channel::aws_eventstream::{SmithyFrame, SmithyFrameParser};

use super::converse::{push_sse as push, stream_index as index};

#[path = "stream_events.rs"]
mod events;

pub(super) struct ConverseStreamDecoder {
    parser: SmithyFrameParser,
    started: bool,
    stopped: bool,
    message_stopped: bool,
    metadata_seen: bool,
    stop_reason: Option<Value>,
    usage: Option<Value>,
    service_tier: Option<Value>,
    blocks: BTreeSet<u64>,
    tools: BTreeMap<u64, ToolBlock>,
}

#[derive(Default)]
struct ToolBlock {
    id: String,
    name: String,
    input: String,
}

impl ConverseStreamDecoder {
    pub(super) fn new() -> Self {
        Self {
            parser: SmithyFrameParser::new(),
            started: false,
            stopped: false,
            message_stopped: false,
            metadata_seen: false,
            stop_reason: None,
            usage: None,
            service_tier: None,
            blocks: BTreeSet::new(),
            tools: BTreeMap::new(),
        }
    }

    fn handle(&mut self, frame: SmithyFrame, out: &mut Vec<u8>) {
        if let Some(kind) = frame.exception_type {
            self.stopped = true;
            let message = frame
                .payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("AWS Bedrock stream failed");
            push(
                out,
                "error",
                json!({
                    "type": "error",
                    "error": { "type": kind, "message": message }
                }),
            );
            return;
        }
        let Some(event) = frame.event_type.as_deref() else {
            return;
        };
        match event {
            "messageStart" => self.message_start(out),
            "contentBlockStart" => self.content_start(&frame.payload, out),
            "contentBlockDelta" => self.content_delta(&frame.payload, out),
            "contentBlockStop" => {
                let index = index(&frame.payload);
                if let Some(tool) = self.tools.remove(&index) {
                    let input =
                        serde_json::from_str::<Value>(&tool.input).unwrap_or_else(|_| json!({}));
                    push(
                        out,
                        "content_block_start",
                        json!({
                            "type": "content_block_start", "index": index,
                            "content_block": {
                                "type": "tool_use", "id": tool.id,
                                "name": tool.name, "input": input
                            }
                        }),
                    );
                }
                if self.blocks.remove(&index) {
                    push(
                        out,
                        "content_block_stop",
                        json!({ "type": "content_block_stop", "index": index }),
                    );
                }
            }
            "messageStop" => {
                self.stop_reason = frame.payload.get("stopReason").cloned();
                self.message_stopped = true;
                if self.metadata_seen {
                    let usage = self.usage.take();
                    self.finish_message(usage, out);
                }
            }
            "metadata" => {
                self.metadata_seen = true;
                self.usage = frame.payload.get("usage").cloned();
                self.service_tier = frame.payload.get("serviceTier").cloned();
                if self.message_stopped {
                    let usage = self.usage.take();
                    self.finish_message(usage, out);
                }
            }
            _ => {}
        }
    }

    fn message_start(&mut self, out: &mut Vec<u8>) {
        if self.started {
            return;
        }
        self.started = true;
        push(
            out,
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": format!("msg_{}", crate::util::id::ulid().to_ascii_lowercase()),
                    "type": "message", "role": "assistant", "model": "aws-bedrock",
                    "content": [], "stop_reason": null, "stop_sequence": null,
                    "usage": {}
                }
            }),
        );
    }

    fn finish_message(&mut self, usage: Option<Value>, out: &mut Vec<u8>) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        let mut usage = usage
            .map(super::converse::usage)
            .unwrap_or_else(|| json!({}));
        super::converse::apply_service_tier(&mut usage, self.service_tier.take());
        push(
            out,
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": super::converse::stop_reason(self.stop_reason.take()),
                    "stop_sequence": null
                },
                "usage": usage
            }),
        );
        push(out, "message_stop", json!({ "type": "message_stop" }));
    }
}

impl ChannelStreamDecoder for ConverseStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, crate::channel::transport::ClientError> {
        let mut out = Vec::new();
        for frame in self.parser.push(chunk) {
            self.handle(frame, &mut out);
        }
        Ok(out)
    }

    fn finish(&mut self) -> Result<Vec<u8>, crate::channel::transport::ClientError> {
        let mut out = Vec::new();
        if self.parser.has_pending() {
            return Err(crate::channel::transport::ClientError::Decode(
                "AWS Bedrock stream ended inside a frame".to_owned(),
            ));
        } else if self.started && !self.stopped {
            let usage = self.usage.take();
            self.finish_message(usage, &mut out);
        }
        Ok(out)
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
