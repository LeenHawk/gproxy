use super::common::{StreamLimits, bound, invalid, limit};
use crate::{
    transform::{
        TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        claude::{generate_content as c, stream as s},
        gemini as g,
    },
};
use std::collections::BTreeMap;
enum Payload {
    Text {
        value: String,
        thought: bool,
        emitted: bool,
    },
    Tool {
        id: String,
        name: String,
        input: serde_json::Map<String, serde_json::Value>,
        json: String,
    },
    Omitted,
}
struct Block {
    payload: Payload,
    closed: bool,
    bytes: usize,
}
pub(super) struct Blocks {
    values: BTreeMap<i64, Block>,
    cursor: i64,
    total_blocks: usize,
    tool_count: usize,
    pending_bytes: usize,
    limits: StreamLimits,
    calls: super::super::history::Calls,
}
impl Blocks {
    pub fn new(limits: StreamLimits) -> Self {
        Self {
            values: BTreeMap::new(),
            cursor: 0,
            total_blocks: 0,
            tool_count: 0,
            pending_bytes: 0,
            limits,
            calls: Default::default(),
        }
    }
    pub fn start(
        &mut self,
        event: s::ContentBlockStartEvent,
        flow: &mut IdentityFlow,
        policy: &TargetIdPolicy,
    ) -> Result<(), TransformError> {
        if self.total_blocks >= self.limits.max_blocks {
            return Err(limit());
        }
        self.total_blocks += 1;
        let (payload, bytes) = match event.content_block {
            c::ResponseContentBlock::Text(v) => {
                let n = v.text.len();
                (
                    Payload::Text {
                        value: v.text,
                        thought: false,
                        emitted: false,
                    },
                    n,
                )
            }
            c::ResponseContentBlock::Thinking(v) => {
                let n = v.thinking.len();
                (
                    Payload::Text {
                        value: v.thinking,
                        thought: true,
                        emitted: false,
                    },
                    n,
                )
            }
            c::ResponseContentBlock::RedactedThinking(_) => (Payload::Omitted, 0),
            c::ResponseContentBlock::ToolUse(v) => {
                if self.tool_count >= self.limits.max_tools {
                    return Err(limit());
                }
                super::super::results::direct_caller(v.caller.flatten())?;
                let id =
                    self.calls
                        .call(Some(v.id), &v.name, crate::Dialect::Claude, flow, policy)?;
                self.tool_count += 1;
                let n = bound(&v.input, self.limits.max_pending)?
                    .checked_add(v.name.len())
                    .and_then(|n| n.checked_add(id.len()))
                    .ok_or_else(limit)?;
                (
                    Payload::Tool {
                        id,
                        name: v.name,
                        input: v.input,
                        json: String::new(),
                    },
                    n,
                )
            }
            _ => {
                return Err(TransformError::unsupported(
                    "content_block",
                    "native execution, resources and continuation require an invocation adapter",
                ));
            }
        };
        let pending = self.pending_bytes.checked_add(bytes).ok_or_else(limit)?;
        if (event.index != self.cursor || matches!(payload, Payload::Tool { .. }))
            && pending > self.limits.max_pending
        {
            return Err(limit());
        }
        self.pending_bytes = pending;
        self.values.insert(
            event.index,
            Block {
                payload,
                closed: false,
                bytes,
            },
        );
        Ok(())
    }
    pub fn delta(&mut self, event: s::ContentBlockDeltaEvent) -> Result<(), TransformError> {
        let block = self
            .values
            .get_mut(&event.index)
            .ok_or_else(|| invalid("content_block_delta", "unknown block"))?;
        let bytes = match &event.delta {
            s::ContentBlockDelta::Text(v) => v.text.len(),
            s::ContentBlockDelta::Thinking(v) => v.thinking.len(),
            s::ContentBlockDelta::InputJson(v) => v.partial_json.len(),
            _ => 0,
        };
        let pending = self.pending_bytes.checked_add(bytes).ok_or_else(limit)?;
        if (event.index != self.cursor || matches!(block.payload, Payload::Tool { .. }))
            && pending > self.limits.max_pending
        {
            return Err(limit());
        }
        let added = match (&mut block.payload, event.delta) {
            (Payload::Text { value, .. }, s::ContentBlockDelta::Text(v)) => {
                let n = v.text.len();
                value.push_str(&v.text);
                n
            }
            (Payload::Text { value, .. }, s::ContentBlockDelta::Thinking(v)) => {
                let n = v.thinking.len();
                value.push_str(&v.thinking);
                n
            }
            (Payload::Tool { json, .. }, s::ContentBlockDelta::InputJson(v)) => {
                let n = v.partial_json.len();
                json.push_str(&v.partial_json);
                n
            }
            (_, s::ContentBlockDelta::Citations(_) | s::ContentBlockDelta::Signature(_)) => 0,
            _ => {
                return Err(invalid(
                    "content_block_delta",
                    "payload kind differs from block",
                ));
            }
        };
        block.bytes = block.bytes.checked_add(added).ok_or_else(limit)?;
        self.pending_bytes = self.pending_bytes.checked_add(added).ok_or_else(limit)?;
        Ok(())
    }
    pub fn stop(&mut self, index: i64) -> Result<(), TransformError> {
        self.values
            .get_mut(&index)
            .ok_or_else(|| invalid("content_block_stop", "unknown block"))?
            .closed = true;
        Ok(())
    }
    /// Return at most one part at a time so the caller meters it before retaining
    /// a batch. Later blocks cannot overtake an earlier open native block.
    pub fn next(&mut self) -> Result<Option<g::Part>, TransformError> {
        loop {
            let Some(block) = self.values.get_mut(&self.cursor) else {
                return Ok(None);
            };
            match &mut block.payload {
                Payload::Text {
                    value,
                    thought,
                    emitted,
                } => {
                    if !value.is_empty() || (block.closed && !*emitted) {
                        let mut part = g::Part::builder().text(std::mem::take(value)).build();
                        if *thought {
                            part.thought = Some(true);
                        }
                        *emitted = true;
                        self.pending_bytes -= block.bytes;
                        block.bytes = 0;
                        return Ok(Some(part));
                    }
                    // An empty start/delta owns no retained text bytes.
                    self.pending_bytes -= block.bytes;
                    block.bytes = 0;
                }
                Payload::Tool { .. } if !block.closed => return Ok(None),
                Payload::Tool { .. } => {
                    let block = self.values.remove(&self.cursor).unwrap();
                    self.pending_bytes -= block.bytes;
                    self.cursor = self.cursor.checked_add(1).ok_or_else(limit)?;
                    let Payload::Tool {
                        id,
                        name,
                        input,
                        json,
                    } = block.payload
                    else {
                        unreachable!()
                    };
                    let args = if json.is_empty() {
                        input
                    } else {
                        serde_json::from_str(&json)
                            .map_err(|e| invalid("tool.input", e.to_string()))?
                    };
                    return Ok(Some(
                        g::Part::builder()
                            .function_call(g::FunctionCall::builder(name).id(id).args(args).build())
                            .build(),
                    ));
                }
                Payload::Omitted => {}
            }
            if !block.closed {
                return Ok(None);
            }
            let block = self.values.remove(&self.cursor).unwrap();
            self.pending_bytes -= block.bytes;
            self.cursor = self.cursor.checked_add(1).ok_or_else(limit)?;
        }
    }
    pub fn check_pending(&self) -> Result<(), TransformError> {
        if self.pending_bytes > self.limits.max_pending {
            Err(limit())
        } else {
            Ok(())
        }
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
