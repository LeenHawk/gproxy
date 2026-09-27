use super::{
    common::{invalid, limit, measure},
    responses_to_claude::ResponsesToClaudeStream,
};
use crate::{
    transform::TransformError,
    wire::{
        claude::{content as cc, stream as cs},
        openai::responses::{input as i, response as r, stream as s},
    },
};
use std::collections::BTreeMap;

pub(super) struct Part {
    pub pending: String,
    pub refusal: bool,
    pub block: Option<i64>,
    pub done: bool,
    pub closed: bool,
}

pub(super) enum Kind {
    Excluded,
    Message {
        parts: BTreeMap<i64, Part>,
    },
    Function {
        call_id: String,
        name: String,
        caller: Option<Option<cc::Caller>>,
        pending: String,
        block: Option<i64>,
        arguments_done: bool,
        closed: bool,
    },
    /// Responses reasoning has no Claude form: a Claude thinking block needs
    /// a signature only Anthropic issues. It is consumed without output.
    Reasoning,
    Mcp {
        final_item: Option<i::McpCall>,
        projected: bool,
    },
}

pub(super) struct Item {
    pub id: Option<String>,
    pub kind: Kind,
    pub done: bool,
    pub held: usize,
}

impl ResponsesToClaudeStream {
    pub(super) fn add_item(
        &mut self,
        index: i64,
        value: r::ResponseOutputItem,
    ) -> Result<(), TransformError> {
        if self.items.len() >= self.limits.max_items {
            return Err(limit());
        }
        let (id, kind, held) = match value {
            r::ResponseOutputItem::Message(v) => {
                let mut parts = BTreeMap::new();
                let mut bytes = 0usize;
                for (n, part) in v.content.into_iter().enumerate() {
                    self.count_part()?;
                    let (text, refusal) = match part {
                        i::OutputContent::Text(v) => (v.text, false),
                        i::OutputContent::Refusal(v) => (v.refusal, true),
                    };
                    bytes = bytes.checked_add(text.len()).ok_or_else(limit)?;
                    parts.insert(
                        n as i64,
                        Part {
                            pending: text,
                            refusal,
                            block: None,
                            done: false,
                            closed: false,
                        },
                    );
                }
                (Some(v.id), Kind::Message { parts }, bytes)
            }
            r::ResponseOutputItem::FunctionCall(v)
                if v.namespace.is_some()
                    || v.caller
                        .as_ref()
                        .and_then(Option::as_ref)
                        .is_some_and(|v| matches!(v, i::Caller::Program(_))) =>
            {
                (v.id, Kind::Excluded, 0)
            }
            r::ResponseOutputItem::FunctionCall(v) => {
                self.count_tool()?;
                if v.call_id.is_empty() || v.name.is_empty() {
                    return Err(invalid("missing native function call ID/name"));
                }
                let caller = v.caller.map(|v| {
                    v.and_then(|v| match v {
                        i::Caller::Direct(_) => Some(cc::Caller::Direct(cc::DirectCaller {
                            rest: Default::default(),
                        })),
                        i::Caller::Program(_) => None,
                    })
                });
                let held = v.arguments.len();
                (
                    v.id,
                    Kind::Function {
                        call_id: v.call_id,
                        name: v.name,
                        caller,
                        pending: v.arguments,
                        block: None,
                        arguments_done: false,
                        closed: false,
                    },
                    held,
                )
            }
            r::ResponseOutputItem::Reasoning(v) => {
                for _ in 0..v.summary.len() + v.content.as_ref().map_or(0, Vec::len) {
                    self.count_part()?;
                }
                (Some(v.id), Kind::Reasoning, 0)
            }
            r::ResponseOutputItem::McpCall(v) => {
                self.count_tool()?;
                (
                    Some(v.id),
                    Kind::Mcp {
                        final_item: None,
                        projected: false,
                    },
                    0,
                )
            }
            _ => (None, Kind::Excluded, 0),
        };
        self.reserve_held(held)?;
        if self
            .items
            .insert(
                index,
                Item {
                    id,
                    kind,
                    done: false,
                    held,
                },
            )
            .is_some()
        {
            return Err(invalid("duplicate source item"));
        }
        Ok(())
    }
    pub(super) fn add_part(
        &mut self,
        index: i64,
        part_index: i64,
        value: s::OutputContentPart,
    ) -> Result<(), TransformError> {
        if self
            .items
            .get(&index)
            .is_some_and(|item| matches!(item.kind, Kind::Excluded))
        {
            return Ok(());
        }

        self.count_part()?;
        let (text, refusal) = match value {
            s::OutputContentPart::Text(v) => (v.text, false),
            s::OutputContentPart::Refusal(v) => (v.refusal, true),
            s::OutputContentPart::Reasoning(_) => return Ok(()),
        };
        let bytes = text.len();
        self.reserve_held(bytes)?;
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("part without item"))?;
        let Kind::Message { parts } = &mut item.kind else {
            return Err(invalid("text part on non-message"));
        };
        item.held += bytes;
        if parts
            .insert(
                part_index,
                Part {
                    pending: text,
                    refusal,
                    block: None,
                    done: false,
                    closed: false,
                },
            )
            .is_some()
        {
            return Err(invalid("duplicate content part"));
        }
        Ok(())
    }
    pub(super) fn bind(&mut self, index: i64, id: Option<&str>) -> Result<(), TransformError> {
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("missing source item"))?;
        if let Some(id) = id {
            if item.id.as_deref().is_some_and(|old| old != id) {
                return Err(invalid("item ID differs from call association"));
            }
            item.id = Some(id.to_owned());
        }
        Ok(())
    }
    pub(super) fn append_text(
        &mut self,
        index: i64,
        part_index: i64,
        id: &str,
        value: String,
        refusal: bool,
        out: &mut Vec<cs::StreamEvent>,
    ) -> Result<(), TransformError> {
        if self
            .items
            .get(&index)
            .is_some_and(|item| matches!(item.kind, Kind::Excluded))
        {
            return Ok(());
        }

        self.bind(index, Some(id))?;
        let block = match self.items.get(&index).map(|v| &v.kind) {
            Some(Kind::Message { parts }) => {
                let p = parts
                    .get(&part_index)
                    .ok_or_else(|| invalid("delta without content part"))?;
                if p.refusal != refusal {
                    return Err(invalid("text/refusal kind mismatch"));
                }
                p.block
            }
            _ => return Err(invalid("text delta on non-message")),
        };
        if let Some(block) = block {
            self.emit(
                out,
                cs::StreamEvent::ContentBlockDelta(
                    cs::ContentBlockDeltaEvent::builder(
                        block,
                        cs::ContentBlockDelta::Text(cs::TextDelta::builder(value).build()),
                    )
                    .build(),
                ),
            )
        } else {
            let n = value.len();
            self.reserve_held(n)?;
            let item = self.items.get_mut(&index).unwrap();
            item.held += n;
            let Kind::Message { parts } = &mut item.kind else {
                // This branch is reached only for an item recorded as Kind::Message.
                unreachable!()
            };
            parts.get_mut(&part_index).unwrap().pending.push_str(&value);
            Ok(())
        }
    }
    pub(super) fn append_args(
        &mut self,
        index: i64,
        id: &str,
        value: String,
        out: &mut Vec<cs::StreamEvent>,
    ) -> Result<(), TransformError> {
        if self
            .items
            .get(&index)
            .is_some_and(|item| matches!(item.kind, Kind::Excluded))
        {
            return Ok(());
        }

        self.bind(index, Some(id))?;
        let block = match self.items.get(&index).map(|v| &v.kind) {
            Some(Kind::Function { block, .. }) => *block,
            Some(Kind::Mcp { .. }) => return Ok(()),
            _ => return Err(invalid("arguments on non-call")),
        };
        if let Some(block) = block {
            self.emit(
                out,
                cs::StreamEvent::ContentBlockDelta(
                    cs::ContentBlockDeltaEvent::builder(
                        block,
                        cs::ContentBlockDelta::InputJson(
                            cs::InputJsonDelta::builder(value).build(),
                        ),
                    )
                    .build(),
                ),
            )
        } else {
            let n = value.len();
            self.reserve_held(n)?;
            let item = self.items.get_mut(&index).unwrap();
            item.held += n;
            let Kind::Function { pending, .. } = &mut item.kind else {
                // This branch is reached only for an item recorded as Kind::Function.
                unreachable!()
            };
            pending.push_str(&value);
            Ok(())
        }
    }
    pub(super) fn args_done(
        &mut self,
        index: i64,
        value: &str,
        out: &mut Vec<cs::StreamEvent>,
    ) -> Result<(), TransformError> {
        if self
            .items
            .get(&index)
            .is_some_and(|item| matches!(item.kind, Kind::Excluded))
        {
            return Ok(());
        }

        let object: serde_json::Map<String, serde_json::Value> = serde_json::from_str(value)
            .map_err(|e| invalid(format!("Claude function input requires an object: {e}")))?;
        drop(object);
        if let Some(Item {
            kind:
                Kind::Function {
                    arguments_done,
                    block,
                    closed,
                    ..
                },
            ..
        }) = self.items.get_mut(&index)
        {
            *arguments_done = true;
            let close = block.filter(|_| !*closed);
            if close.is_some() {
                *closed = true;
            }
            if let Some(block) = close {
                self.emit(
                    out,
                    cs::StreamEvent::ContentBlockStop(
                        cs::ContentBlockStopEvent::builder(block).build(),
                    ),
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn part_done(
        &mut self,
        index: i64,
        part_index: i64,
        out: &mut Vec<cs::StreamEvent>,
    ) -> Result<(), TransformError> {
        if self
            .items
            .get(&index)
            .is_some_and(|item| matches!(item.kind, Kind::Excluded))
        {
            return Ok(());
        }

        let close = if let Some(Item {
            kind: Kind::Message { parts },
            ..
        }) = self.items.get_mut(&index)
        {
            let part = parts
                .get_mut(&part_index)
                .ok_or_else(|| invalid("part done without start"))?;
            part.done = true;
            let block = part.block.filter(|_| !part.closed);
            if block.is_some() {
                part.closed = true;
            }
            block
        } else {
            None
        };
        if let Some(block) = close {
            self.emit(
                out,
                cs::StreamEvent::ContentBlockStop(
                    cs::ContentBlockStopEvent::builder(block).build(),
                ),
            )?;
        }
        Ok(())
    }
    pub(super) fn item_done(
        &mut self,
        index: i64,
        value: r::ResponseOutputItem,
        out: &mut Vec<cs::StreamEvent>,
    ) -> Result<(), TransformError> {
        if let Some(item) = self.items.get_mut(&index)
            && matches!(item.kind, Kind::Excluded)
        {
            item.done = true;
            return Ok(());
        }

        self.bind(index, super::common::item_id(&value))?;
        match value {
            r::ResponseOutputItem::Message(_) => {
                let keys = match &self
                    .items
                    .get(&index)
                    .ok_or_else(|| invalid("item done without item"))?
                    .kind
                {
                    Kind::Message { parts } => parts.keys().copied().collect::<Vec<_>>(),
                    _ => return Err(invalid("message kind mismatch")),
                };
                for key in keys {
                    self.part_done(index, key, out)?;
                }
            }
            r::ResponseOutputItem::FunctionCall(v) => self.args_done(index, &v.arguments, out)?,
            r::ResponseOutputItem::McpCall(v) => {
                let bytes = measure(&v, self.limits.max_pending)?;
                self.reserve_held(bytes)?;
                let item = self.items.get_mut(&index).unwrap();
                item.held += bytes;
                let Kind::Mcp { final_item, .. } = &mut item.kind else {
                    return Err(invalid("MCP kind mismatch"));
                };
                *final_item = Some(v);
            }
            _ => {}
        }
        self.items.get_mut(&index).unwrap().done = true;
        Ok(())
    }

    fn count_part(&mut self) -> Result<(), TransformError> {
        if self.parts >= self.limits.max_parts {
            return Err(limit());
        }
        self.parts += 1;
        Ok(())
    }
    fn count_tool(&mut self) -> Result<(), TransformError> {
        if self.tools >= self.limits.max_tools {
            return Err(limit());
        }
        self.tools += 1;
        Ok(())
    }
}
