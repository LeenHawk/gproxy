//! Kiro `toolUseEvent` → OpenAI Responses **function-call** streaming state.
//!
//! Kiro streams a tool call as a series of `toolUseEvent` frames sharing one
//! `toolUseId`: `input` carries PARTIAL JSON-argument fragments that must be
//! ACCUMULATED, and `stop == true` marks the last fragment. We map each tool
//! call onto the Responses function-call lifecycle:
//!
//! ```text
//!   first fragment  → response.output_item.added (function_call, arguments:"")
//!   every fragment  → response.function_call_arguments.delta (fragment)
//!   stop == true    → response.function_call_arguments.done  (full arguments)
//!                     response.output_item.done             (full arguments)
//! ```
//!
//! Output indices follow text (0) and reasoning (1): the first tool call lands
//! at index 2, the next at 3, and so on. Multiple concurrent/sequential calls
//! are tracked by `toolUseId`. Calls that never received `stop` are flushed in
//! [`ToolCallTracker::finish`] so the Responses stream always closes cleanly.
//!
//! Fully sync — compiles on the wasm edge target.

use serde_json::{Value, json};

use super::sse::{gen_id, push_sse};

/// First output_index available to tool calls (0 = text, 1 = reasoning).
const TOOL_INDEX_BASE: u64 = 2;

/// Per-tool-call accumulation state.
struct ToolCall {
    item_id: String,
    output_index: u64,
    name: String,
    call_id: String,
    arguments: String,
    done: bool,
}

/// Tracks every tool call seen in one response, keyed by `toolUseId`.
#[derive(Default)]
pub(super) struct ToolCallTracker {
    /// Insertion-ordered so output indices stay stable across fragments.
    calls: Vec<ToolCall>,
}

impl ToolCallTracker {
    /// Handle one `toolUseEvent` payload. `seq` is the shared decoder sequence
    /// counter (mutated in place to keep `sequence_number` monotonic with the
    /// surrounding text/reasoning events).
    pub(super) fn handle(&mut self, payload: &Value, seq: &mut u64, out: &mut Vec<u8>) {
        let Some(tool_use_id) = payload.get("toolUseId").and_then(Value::as_str) else {
            return;
        };
        let name = payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let fragment = payload
            .get("input")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let stop = payload
            .get("stop")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let idx = match self.calls.iter().position(|c| c.call_id == tool_use_id) {
            Some(idx) => idx,
            None => {
                let output_index = TOOL_INDEX_BASE + self.calls.len() as u64;
                let item_id = gen_id("fc");
                self.calls.push(ToolCall {
                    item_id: item_id.clone(),
                    output_index,
                    name: name.clone(),
                    call_id: tool_use_id.to_string(),
                    arguments: String::new(),
                    done: false,
                });
                let s = take_seq(seq);
                push_sse(
                    out,
                    json!({
                        "type": "response.output_item.added",
                        "sequence_number": s,
                        "output_index": output_index,
                        "item": {
                            "type": "function_call",
                            "id": item_id,
                            "call_id": tool_use_id,
                            "name": name,
                            "arguments": "",
                        },
                    }),
                );
                self.calls.len() - 1
            }
        };

        // A late `name` (the first fragment may carry it empty) wins.
        if self.calls[idx].name.is_empty() && !name.is_empty() {
            self.calls[idx].name = name;
        }

        if !fragment.is_empty() {
            self.calls[idx].arguments.push_str(&fragment);
            let (item_id, output_index) = {
                let c = &self.calls[idx];
                (c.item_id.clone(), c.output_index)
            };
            let s = take_seq(seq);
            push_sse(
                out,
                json!({
                    "type": "response.function_call_arguments.delta",
                    "sequence_number": s,
                    "output_index": output_index,
                    "item_id": item_id,
                    "delta": fragment,
                }),
            );
        }

        if stop {
            self.emit_done(idx, seq, out);
        }
    }

    /// Emit `function_call_arguments.done` + `output_item.done` for one call.
    fn emit_done(&mut self, idx: usize, seq: &mut u64, out: &mut Vec<u8>) {
        if self.calls[idx].done {
            return;
        }
        self.calls[idx].done = true;
        let (item_id, output_index, name, call_id, arguments) = {
            let c = &self.calls[idx];
            (
                c.item_id.clone(),
                c.output_index,
                c.name.clone(),
                c.call_id.clone(),
                c.arguments.clone(),
            )
        };
        let s = take_seq(seq);
        push_sse(
            out,
            json!({
                "type": "response.function_call_arguments.done",
                "sequence_number": s,
                "output_index": output_index,
                "item_id": item_id,
                "arguments": arguments,
            }),
        );
        let s = take_seq(seq);
        push_sse(
            out,
            json!({
                "type": "response.output_item.done",
                "sequence_number": s,
                "output_index": output_index,
                "item": {
                    "type": "function_call",
                    "id": item_id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments,
                },
            }),
        );
    }

    /// Close any tool call whose `stop` never arrived (upstream cut short).
    pub(super) fn finish(&mut self, seq: &mut u64, out: &mut Vec<u8>) {
        for idx in 0..self.calls.len() {
            if !self.calls[idx].done {
                self.emit_done(idx, seq, out);
            }
        }
    }

    /// Completed function-call items for the final `response.completed` output.
    pub(super) fn completed_items(&self) -> Vec<Value> {
        self.calls
            .iter()
            .map(|c| {
                json!({
                    "type": "function_call",
                    "id": c.item_id,
                    "call_id": c.call_id,
                    "name": c.name,
                    "arguments": c.arguments,
                    "status": "completed",
                })
            })
            .collect()
    }
}

/// Read the shared sequence counter and advance it.
fn take_seq(seq: &mut u64) -> u64 {
    let s = *seq;
    *seq += 1;
    s
}
