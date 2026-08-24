use gproxy_channel_api::{ChannelError, Frame};
use serde_json::{Value, json};

use super::codec::Codec;
use super::sse::encode;

impl Codec {
    pub(super) fn modern(
        &mut self,
        value: &mut Value,
        frames: &mut Vec<Frame>,
    ) -> Result<Option<String>, ChannelError> {
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .expect("type-less events are routed to the legacy decoder")
            .to_owned();
        let index = value.get("index").and_then(Value::as_u64);
        if kind == "content_block_start" {
            match value.pointer("/content_block/type").and_then(Value::as_str) {
                Some("tool_result") => {
                    self.skipped_result = index;
                    return Ok(None);
                }
                Some("tool_use") => {
                    self.tool_index = index;
                    self.tool_id = value
                        .pointer("/content_block/id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                _ => {}
            }
        }
        if self.skipped_result == index {
            if kind == "content_block_stop" {
                self.skipped_result = None;
            }
            return Ok(None);
        }
        match kind.as_str() {
            "message_start" => {
                self.started = true;
                if let Some(message) = value.get_mut("message").and_then(Value::as_object_mut) {
                    message.insert(
                        "usage".into(),
                        json!({"input_tokens":self.state.input_tokens,"output_tokens":0}),
                    );
                    if let Some(id) = message.get("id").and_then(Value::as_str) {
                        self.state.message_id = id.into();
                    }
                }
            }
            "content_block_delta" => self.count_delta(value),
            "message_delta" => {
                value["usage"] = json!({"output_tokens":self.output_tokens()});
            }
            "message_stop" => self.stopped = true,
            _ => {}
        }
        frames.push(Frame(encode(value)));
        if kind == "content_block_stop" && self.tool_index == index {
            self.tool_index = None;
            self.state.input_tokens = self.state.input_tokens.saturating_add(self.output_tokens());
            frames.push(Frame(encode(&json!({
                "type":"message_delta",
                "delta":{"stop_reason":"tool_use","stop_sequence":null},
                "usage":{"output_tokens":self.output_tokens()}
            }))));
            frames.push(Frame(encode(&json!({"type":"message_stop"}))));
            self.stopped = true;
            return self
                .tool_id
                .take()
                .map(Some)
                .ok_or_else(|| ChannelError::Decode("tool_use id missing".into()));
        }
        Ok(None)
    }

    fn count_delta(&mut self, value: &Value) {
        if let Some(text) = value
            .get("delta")
            .and_then(|delta| {
                delta
                    .get("text")
                    .or_else(|| delta.get("thinking"))
                    .or_else(|| delta.get("partial_json"))
            })
            .and_then(Value::as_str)
        {
            self.output.push_str(text);
        }
    }
}
