use bytes::Bytes;
use gproxy_protocol::ContentGenerationKind;
use gproxy_transform::{ResponseCollector, SseDecoder, SseFrame};
use serde_json::{Value, json};

use crate::error::CoreError;

pub(super) struct Events {
    decoder: SseDecoder,
    collector: Option<ResponseCollector>,
    prefix: Vec<Bytes>,
    terminal: Vec<Value>,
    handoff: Option<Value>,
    started: bool,
    offset: u64,
    next_index: u64,
    pub output: bool,
}

impl Events {
    pub(super) fn new() -> Self {
        Self {
            decoder: Default::default(),
            collector: Some(collector()),
            prefix: Vec::new(),
            terminal: Vec::new(),
            handoff: None,
            started: false,
            offset: 0,
            next_index: 0,
            output: false,
        }
    }

    pub(super) fn retry(&mut self, boundary: Value) {
        self.decoder = Default::default();
        self.collector = Some(collector());
        self.prefix.clear();
        self.terminal.clear();
        self.output = false;
        if !self.started {
            self.offset = 0;
            self.next_index = 0;
        }
        self.handoff = Some(boundary);
    }

    pub(super) fn push(&mut self, chunk: Bytes) -> Result<Vec<Bytes>, CoreError> {
        let mut output = Vec::new();
        for frame in self.decoder.push(&chunk)? {
            output.extend(self.frame(frame)?);
        }
        Ok(output)
    }

    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, CoreError> {
        let original = SseFrame::encode(frame.event.as_deref(), &frame.data);
        self.collector
            .as_mut()
            .expect("active collector")
            .push(original.clone())?;
        let mut event: Value = serde_json::from_str(&frame.data)
            .map_err(|error| CoreError::Transform(error.to_string()))?;
        let kind = event["type"].as_str().unwrap_or_default().to_owned();
        if kind == "message_delta" && !event["delta"]["stop_reason"].is_null()
            || kind == "message_stop"
        {
            self.terminal.push(event);
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        if kind == "message_start" {
            if let Some(mut boundary) = self.handoff.take() {
                if let Some(model) = event.pointer("/message/model") {
                    boundary["to"]["model"] = model.clone();
                }
                if !self.started {
                    self.prefix.push(original);
                }
                let index = self.next_index;
                self.next_index += 1;
                self.offset = self.next_index;
                output.push(encode(
                    &json!({"type":"content_block_start","index":index,"content_block":boundary}),
                ));
                output.push(encode(&json!({"type":"content_block_stop","index":index})));
            } else {
                self.prefix.push(original);
            }
        } else {
            if let Some(index) = event.get("index").and_then(Value::as_u64) {
                let index = index.checked_add(self.offset).ok_or_else(|| {
                    CoreError::Transform("fallback content index overflow".into())
                })?;
                event["index"] = json!(index);
                self.next_index = self.next_index.max(index.checked_add(1).ok_or_else(|| {
                    CoreError::Transform("fallback content index overflow".into())
                })?);
            }
            output.push(if self.offset == 0 {
                original
            } else {
                encode(&event)
            });
        }
        self.output |= self
            .collector
            .as_ref()
            .expect("active collector")
            .claude_has_output();
        if !self.started {
            self.prefix.extend(output);
            // Buffer metadata only, not the first text/tool/thinking output.
            if self.output
                || kind == "ping"
                || kind == "error"
                || self.prefix.iter().map(Bytes::len).sum::<usize>() >= 16 * 1024
            {
                self.started = true;
                Ok(std::mem::take(&mut self.prefix))
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(output)
        }
    }

    pub(super) fn end(&mut self) -> Result<(Vec<Bytes>, Value, bool), CoreError> {
        let frames = if let Some(frame) = self.decoder.finish()? {
            self.frame(frame)?
        } else {
            Vec::new()
        };
        let collector = self.collector.take().expect("active collector");
        let open_tool = collector.claude_has_open_tool();
        let response = serde_json::from_slice(&collector.finish()?.into_bytes()?)
            .map_err(|error| CoreError::Transform(error.to_string()))?;
        Ok((frames, response, open_tool))
    }

    pub(super) fn finish(&mut self, response: &Value, iterations: &[Value]) -> Vec<Bytes> {
        let mut output = std::mem::take(&mut self.prefix);
        for mut event in self.terminal.drain(..) {
            if event["type"] == "message_delta" {
                event["delta"]["stop_details"] = response["stop_details"].clone();
                if !iterations.is_empty() {
                    event["usage"] = response["usage"].clone();
                    if iterations.len() > 1 {
                        let mut iterations = iterations.to_vec();
                        for item in &mut iterations {
                            item["type"] = json!("message");
                        }
                        iterations.last_mut().expect("nonempty")["type"] =
                            json!("fallback_message");
                        event["usage"]["iterations"] = json!(iterations);
                    }
                }
            }
            output.push(encode(&event));
        }
        output
    }
}

fn collector() -> ResponseCollector {
    ResponseCollector::new(ContentGenerationKind::ClaudeMessages).expect("Claude collector")
}
fn encode(event: &Value) -> Bytes {
    SseFrame::encode(event["type"].as_str(), &event.to_string())
}
