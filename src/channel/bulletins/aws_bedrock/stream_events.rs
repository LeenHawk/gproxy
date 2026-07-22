use serde_json::{Value, json};

use super::super::converse::{push_sse as push, stream_index as index};
use super::ConverseStreamDecoder;

impl ConverseStreamDecoder {
    pub(super) fn content_start(&mut self, payload: &Value, out: &mut Vec<u8>) {
        self.message_start(out);
        let index = index(payload);
        if !self.blocks.insert(index) {
            return;
        }
        let start = payload.get("start").unwrap_or(&Value::Null);
        if let Some(tool) = start.get("toolUse") {
            self.tools.insert(
                index,
                super::ToolBlock {
                    id: tool
                        .get("toolUseId")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_owned(),
                    name: tool
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_owned(),
                    input: String::new(),
                },
            );
            return;
        }
        let block = if start.get("reasoningContent").is_some() {
            json!({ "type": "thinking", "thinking": "" })
        } else {
            json!({ "type": "text", "text": "" })
        };
        push(
            out,
            "content_block_start",
            json!({
                "type": "content_block_start", "index": index, "content_block": block
            }),
        );
    }

    pub(super) fn content_delta(&mut self, payload: &Value, out: &mut Vec<u8>) {
        let delta = payload.get("delta").unwrap_or(&Value::Null);
        let index = index(payload);
        if let Some(tool) = delta.get("toolUse") {
            let state = self.tools.entry(index).or_insert_with(|| super::ToolBlock {
                id: "tool".into(),
                name: "tool".into(),
                input: String::new(),
            });
            if let Some(input) = tool.get("input").and_then(Value::as_str) {
                state.input.push_str(input);
            }
            self.blocks.insert(index);
            return;
        }
        if !self.blocks.contains(&index) {
            let start = if delta.get("reasoningContent").is_some() {
                json!({ "reasoningContent": {} })
            } else {
                json!({})
            };
            self.content_start(&json!({ "contentBlockIndex": index, "start": start }), out);
        }
        let mapped = if let Some(text) = delta.get("text") {
            json!({ "type": "text_delta", "text": text })
        } else if let Some(reasoning) = delta.get("reasoningContent") {
            if let Some(text) = reasoning.get("text") {
                json!({ "type": "thinking_delta", "thinking": text })
            } else {
                json!({
                    "type": "signature_delta",
                    "signature": reasoning.get("signature").cloned().unwrap_or(Value::String(String::new()))
                })
            }
        } else {
            return;
        };
        push(
            out,
            "content_block_delta",
            json!({
                "type": "content_block_delta", "index": index, "delta": mapped
            }),
        );
    }
}
