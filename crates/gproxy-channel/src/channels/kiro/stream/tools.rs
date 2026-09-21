//! The function calls a `toolUseEvent` stream builds up, as Responses items.
//!
//! The upstream sends a tool's name once, then its arguments in fragments,
//! then a `stop`. Responses wants an `output_item.added`, a run of
//! `function_call_arguments.delta` and a matching pair of `done` events, so the
//! tracker holds one record per call id and emits as the fragments arrive.

use super::sse;
use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde_json::{Value, json};

/// Content occupies output index 0 and reasoning index 1, so the first tool
/// call is index 2 (v3 `tool_stream.rs`).
const FIRST_TOOL_INDEX: u64 = 2;

struct Call {
    id: String,
    item_id: String,
    name: String,
    arguments: String,
    index: u64,
    done: bool,
}

impl Call {
    fn item(&self, status: Option<&str>) -> Value {
        let mut item = json!({
            "type": "function_call", "id": self.item_id, "call_id": self.id,
            "name": self.name, "arguments": self.arguments,
        });
        if let Some(status) = status {
            item["status"] = Value::String(status.into());
        }
        item
    }
}

#[derive(Default)]
pub(super) struct Tracker {
    calls: Vec<Call>,
}

impl Tracker {
    /// One `toolUseEvent`. `sequence` is the stream's own counter, shared with
    /// the text events so every record is numbered once.
    pub(super) fn handle(
        &mut self,
        value: &Value,
        sequence: &mut u64,
    ) -> Result<Vec<Bytes>, ChannelError> {
        let call_id = value
            .get("toolUseId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| decode("a tool event has no id"))?;
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let fragment = value
            .get("input")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let stop = value.get("stop").and_then(Value::as_bool).unwrap_or(false);
        let mut output = Vec::new();
        let index = match self.calls.iter().position(|call| call.id == call_id) {
            Some(index) => index,
            None => {
                if name.is_empty() {
                    return Err(decode("a tool event starts without a name"));
                }
                let output_index = FIRST_TOOL_INDEX + self.calls.len() as u64;
                let item_id = sse::id("fc", call_id);
                self.calls.push(Call {
                    id: call_id.into(),
                    item_id,
                    name: name.into(),
                    arguments: String::new(),
                    index: output_index,
                    done: false,
                });
                let call = self.calls.last().expect("just pushed");
                output.push(sse::frame(&json!({
                    "type": "response.output_item.added",
                    "sequence_number": take(sequence),
                    "output_index": output_index,
                    "item": call.item(None),
                })));
                self.calls.len() - 1
            }
        };
        if self.calls[index].name.is_empty() && !name.is_empty() {
            self.calls[index].name = name.into();
        }
        if !fragment.is_empty() {
            self.calls[index].arguments.push_str(fragment);
            output.push(sse::frame(&json!({
                "type": "response.function_call_arguments.delta",
                "sequence_number": take(sequence),
                "output_index": self.calls[index].index,
                "item_id": self.calls[index].item_id,
                "delta": fragment,
            })));
        }
        if stop {
            output.extend(self.finish_call(index, sequence));
        }
        Ok(output)
    }

    /// A stream that ended mid-call delivered arguments the client cannot
    /// use, so the translator refuses it rather than handing over a truncated
    /// call.
    pub(super) fn is_complete(&self) -> bool {
        self.calls.iter().all(|call| call.done)
    }

    pub(super) fn items(&self) -> Vec<Value> {
        self.calls
            .iter()
            .map(|call| call.item(Some("completed")))
            .collect()
    }

    fn finish_call(&mut self, index: usize, sequence: &mut u64) -> Vec<Bytes> {
        if self.calls[index].done {
            return Vec::new();
        }
        self.calls[index].done = true;
        let call = &self.calls[index];
        vec![
            sse::frame(&json!({
                "type": "response.function_call_arguments.done",
                "sequence_number": take(sequence), "output_index": call.index,
                "item_id": call.item_id, "arguments": call.arguments,
            })),
            sse::frame(&json!({
                "type": "response.output_item.done",
                "sequence_number": take(sequence), "output_index": call.index,
                "item": call.item(Some("completed")),
            })),
        ]
    }
}

fn take(sequence: &mut u64) -> u64 {
    let current = *sequence;
    *sequence += 1;
    current
}

fn decode(message: &str) -> ChannelError {
    ChannelError::InvalidResponse(format!("Kiro stream: {message}"))
}
