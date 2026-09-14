use super::{
    common::{invalid, limit, measure},
    responses_to_gemini::ResponsesToGeminiStream,
};
use crate::{
    transform::TransformError,
    wire::openai::responses::{input as i, response as r, stream as s},
};
use std::collections::BTreeMap;
pub(super) struct Part {
    pub pending: String,
    pub emitted: bool,
    pub done: bool,
}
pub(super) enum Kind {
    Message {
        parts: BTreeMap<i64, Part>,
        next: i64,
    },
    Reasoning {
        parts: BTreeMap<i64, Part>,
        next: i64,
        has_content: bool,
        signed: bool,
        final_item: Option<i::ReasoningItem>,
        projected: bool,
    },
    Function {
        value: Box<i::FunctionCall>,
        ready: bool,
        emitted: bool,
    },
}
pub(super) struct Item {
    pub id: Option<String>,
    pub kind: Kind,
    pub done: bool,
    pub held: usize,
}
impl ResponsesToGeminiStream {
    pub(super) fn count_part(&mut self) -> Result<(), TransformError> {
        if self.parts >= self.limits.max_parts {
            return Err(limit());
        }
        self.parts += 1;
        Ok(())
    }
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
                    let text = match part {
                        i::OutputContent::Text(v) => v.text,
                        i::OutputContent::Refusal(v) => v.refusal,
                    };
                    bytes = bytes.checked_add(text.len()).ok_or_else(limit)?;
                    parts.insert(
                        n as i64,
                        Part {
                            pending: text,
                            emitted: false,
                            done: false,
                        },
                    );
                }
                (Some(v.id), Kind::Message { parts, next: 0 }, bytes)
            }
            r::ResponseOutputItem::Reasoning(v) => {
                let signed = self.restoration.parts.contains_key(&v.id);
                let has_content = v.content.is_some();
                let mut parts = BTreeMap::new();
                let mut bytes = 0usize;
                for part in &v.summary {
                    self.count_part()?;
                    bytes = bytes.checked_add(part.text.len()).ok_or_else(limit)?;
                }
                for (n, part) in v.content.into_iter().flatten().enumerate() {
                    self.count_part()?;
                    bytes = bytes.checked_add(part.text.len()).ok_or_else(limit)?;
                    parts.insert(
                        n as i64,
                        Part {
                            pending: part.text,
                            emitted: false,
                            done: false,
                        },
                    );
                }
                (
                    Some(v.id),
                    Kind::Reasoning {
                        parts,
                        next: 0,
                        has_content,
                        signed,
                        final_item: None,
                        projected: false,
                    },
                    bytes,
                )
            }
            r::ResponseOutputItem::FunctionCall(v) => {
                if self.tools >= self.limits.max_tools {
                    return Err(limit());
                }
                self.tools += 1;
                if v.call_id.is_empty()
                    || v.name.is_empty()
                    || !self.native_calls.insert(v.call_id.clone())
                {
                    return Err(invalid("empty/duplicate actual function identity"));
                }
                if v.namespace.is_some()
                    || v.caller
                        .as_ref()
                        .and_then(Option::as_ref)
                        .is_some_and(|v| matches!(v, i::Caller::Program(_)))
                {
                    return Err(TransformError::unsupported(
                        "function.scope",
                        "native program/namespace requires invocation adapter",
                    ));
                }
                let held = measure(&v, self.limits.max_pending)?;
                (
                    v.id.clone(),
                    Kind::Function {
                        value: Box::new(v),
                        ready: false,
                        emitted: false,
                    },
                    held,
                )
            }
            _ => {
                return Err(TransformError::unsupported(
                    "output",
                    "hosted/custom execution or media requires invocation adapter",
                ));
            }
        };
        self.reserve(held)?;
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
    pub(super) fn bind(&mut self, index: i64, id: &str) -> Result<(), TransformError> {
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("unknown source item"))?;
        if item.id.as_ref().is_some_and(|old| old != id) {
            return Err(invalid("source item association changed"));
        }
        item.id = Some(id.into());
        Ok(())
    }
    pub(super) fn add_part(
        &mut self,
        index: i64,
        n: i64,
        value: s::OutputContentPart,
    ) -> Result<(), TransformError> {
        self.count_part()?;
        let (text, reasoning) = match value {
            s::OutputContentPart::Text(v) => (v.text, false),
            s::OutputContentPart::Refusal(v) => (v.refusal, false),
            s::OutputContentPart::Reasoning(v) => (v.text, true),
        };
        self.reserve(text.len())?;
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("part without item"))?;
        item.held += text.len();
        let parts = match &mut item.kind {
            Kind::Message { parts, .. } if !reasoning => parts,
            Kind::Reasoning {
                parts, has_content, ..
            } if reasoning => {
                *has_content = true;
                parts
            }
            _ => return Err(invalid("content kind differs from native item")),
        };
        if parts
            .insert(
                n,
                Part {
                    pending: text,
                    emitted: false,
                    done: false,
                },
            )
            .is_some()
        {
            return Err(invalid("duplicate source part"));
        }
        Ok(())
    }
    pub(super) fn append_text(
        &mut self,
        index: i64,
        n: i64,
        text: String,
        reasoning: bool,
    ) -> Result<(), TransformError> {
        self.reserve(text.len())?;
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("delta without item"))?;
        let parts = match &mut item.kind {
            Kind::Message { parts, .. } if !reasoning => parts,
            Kind::Reasoning { parts, .. } if reasoning => parts,
            _ => return Err(invalid("text delta kind mismatch")),
        };
        let part = parts
            .get_mut(&n)
            .ok_or_else(|| invalid("delta without part"))?;
        item.held += text.len();
        part.pending.push_str(&text);
        Ok(())
    }
    pub(super) fn part_done(&mut self, index: i64, n: i64) -> Result<(), TransformError> {
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("part done without item"))?;
        let parts = match &mut item.kind {
            Kind::Message { parts, .. } | Kind::Reasoning { parts, .. } => parts,
            _ => return Err(invalid("part done on function")),
        };
        parts
            .get_mut(&n)
            .ok_or_else(|| invalid("part done without start"))?
            .done = true;
        Ok(())
    }
    pub(super) fn reserve_arguments(
        &mut self,
        index: i64,
        bytes: usize,
    ) -> Result<(), TransformError> {
        self.reserve(bytes)?;
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("arguments without item"))?;
        if !matches!(item.kind, Kind::Function { emitted: false, .. }) {
            return Err(invalid("arguments after native function completion"));
        }
        item.held += bytes;
        Ok(())
    }
    pub(super) fn arguments_done(
        &mut self,
        index: i64,
        args: String,
    ) -> Result<(), TransformError> {
        let mut item = self
            .items
            .remove(&index)
            .ok_or_else(|| invalid("arguments done without item"))?;
        let Kind::Function {
            value,
            ready,
            emitted,
        } = &mut item.kind
        else {
            return Err(invalid("arguments done on non-function"));
        };
        if !*emitted {
            self.release(item.held);
            value.arguments = args;
            item.held = measure(value, self.limits.max_pending)?;
            self.reserve(item.held)?;
            *ready = true;
        }
        self.items.insert(index, item);
        Ok(())
    }
    pub(super) fn summary(&mut self, index: i64, bytes: usize) -> Result<(), TransformError> {
        self.reserve(bytes)?;
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("summary without item"))?;
        if !matches!(item.kind, Kind::Reasoning { .. }) {
            return Err(invalid("summary on non-reasoning item"));
        }
        item.held += bytes;
        Ok(())
    }
    pub(super) fn item_done(
        &mut self,
        index: i64,
        value: r::ResponseOutputItem,
    ) -> Result<(), TransformError> {
        let id = match &value {
            r::ResponseOutputItem::Message(v) => Some(&v.id),
            r::ResponseOutputItem::Reasoning(v) => Some(&v.id),
            r::ResponseOutputItem::FunctionCall(v) => v.id.as_ref(),
            _ => None,
        };
        if let Some(id) = id {
            self.bind(index, id)?;
        }
        if let r::ResponseOutputItem::FunctionCall(call) = &value {
            self.arguments_done(index, call.arguments.clone())?;
        }
        let mut item = self
            .items
            .remove(&index)
            .ok_or_else(|| invalid("item done without start"))?;
        match (&mut item.kind, value) {
            (Kind::Message { parts, .. }, r::ResponseOutputItem::Message(_)) => {
                for part in parts.values_mut() {
                    part.done = true;
                }
            }
            (
                Kind::Reasoning {
                    parts,
                    has_content,
                    signed,
                    final_item,
                    ..
                },
                r::ResponseOutputItem::Reasoning(v),
            ) => {
                for part in parts.values_mut() {
                    part.done = true;
                }
                if *signed || !*has_content {
                    self.release(item.held);
                    parts.clear();
                    item.held = measure(&v, self.limits.max_pending)?;
                    self.reserve(item.held)?;
                    *final_item = Some(v);
                }
            }
            (Kind::Function { .. }, r::ResponseOutputItem::FunctionCall(_)) => {}
            _ => return Err(invalid("item done kind mismatch")),
        }
        item.done = true;
        self.items.insert(index, item);
        Ok(())
    }
}
