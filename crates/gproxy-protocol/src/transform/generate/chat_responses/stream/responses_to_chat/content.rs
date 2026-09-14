use super::*;
impl ResponsesToChatStream {
    pub(super) fn add_item(
        &mut self,
        index: i64,
        item: r::ResponseOutputItem,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        if self.items.len() >= self.limits.max_items {
            return Err(crate::transform::generate::chat_responses::stream::common::limit());
        }
        let (id, kind) = match item {
            r::ResponseOutputItem::Message(v) => {
                if v.phase.is_some() {
                    self.report
                        .omitted("message.phase", "Chat has no phase field");
                }
                let parts = v
                    .content
                    .into_iter()
                    .enumerate()
                    .map(|(index, p)| {
                        let (pending, refusal) = match p {
                            i::OutputContent::Text(v) => (v.text, false),
                            i::OutputContent::Refusal(v) => (v.refusal, true),
                        };
                        (
                            index as i64,
                            TextPart {
                                pending,
                                refusal,
                                done: false,
                                emitted: false,
                            },
                        )
                    })
                    .collect();
                (
                    Some(v.id),
                    ItemKind::Message {
                        parts,
                        cursor: 0,
                        done: false,
                    },
                )
            }
            r::ResponseOutputItem::FunctionCall(v) => {
                if v.namespace.is_some()
                    || v.caller
                        .flatten()
                        .is_some_and(|c| matches!(c, i::Caller::Program(_)))
                {
                    return Err(unsupported(
                        "function.caller",
                        "server-owned/namespaced function requires an invocation adapter",
                    ));
                }
                if self.tools >= self.limits.max_tool_calls {
                    return Err(
                        crate::transform::generate::chat_responses::stream::common::limit(),
                    );
                }
                let tool_index = self.tools as i64;
                self.tools += 1;
                if v.call_id.is_empty() || v.name.is_empty() {
                    return Err(TransformError::missing_metadata("function.call_id/name"));
                }
                let args = v.arguments;
                self.items.insert(
                    index,
                    ItemState {
                        id: v.id,
                        kind: ItemKind::Function {
                            call_id: v.call_id,
                            name: v.name,
                            tool_index,
                            sent: false,
                        },
                    },
                );
                self.emit_tool(index, args, out)?;
                return Ok(());
            }
            r::ResponseOutputItem::Reasoning(v) => {
                self.report.omitted("response.reasoning", "Chat stream has no native reasoning field; opaque replay requires scoped state");
                (Some(v.id), ItemKind::Reasoning)
            }
            _ => {
                return Err(unsupported(
                    "response.output",
                    "native custom tools, server execution and resources require an invocation adapter for Chat streaming",
                ));
            }
        };
        self.items.insert(index, ItemState { id, kind });
        Ok(())
    }
    pub(super) fn add_part(
        &mut self,
        index: i64,
        part_index: i64,
        part: rs::OutputContentPart,
    ) -> Result<(), TransformError> {
        let Some(ItemState {
            kind: ItemKind::Message { parts, .. },
            ..
        }) = self.items.get_mut(&index)
        else {
            return match part {
                rs::OutputContentPart::Reasoning(_) => Ok(()),
                _ => Err(invalid("content part without message")),
            };
        };
        let (pending, refusal) = match part {
            rs::OutputContentPart::Text(v) => (v.text, false),
            rs::OutputContentPart::Refusal(v) => (v.refusal, true),
            rs::OutputContentPart::Reasoning(_) => return Err(invalid("reasoning in message")),
        };
        parts.insert(
            part_index,
            TextPart {
                pending,
                refusal,
                done: false,
                emitted: false,
            },
        );
        Ok(())
    }
    pub(super) fn append_text(
        &mut self,
        index: i64,
        part_index: i64,
        id: &str,
        text: String,
        refusal: bool,
    ) -> Result<(), TransformError> {
        self.bind_item(index, Some(id))?;
        let Some(ItemState {
            kind: ItemKind::Message { parts, .. },
            ..
        }) = self.items.get_mut(&index)
        else {
            return Err(invalid("text without message"));
        };
        let part = parts
            .get_mut(&part_index)
            .ok_or_else(|| invalid("text without part"))?;
        if part.refusal != refusal {
            return Err(invalid("text/refusal kind changed"));
        }
        part.pending.push_str(&text);
        Ok(())
    }
    pub(super) fn bind_item(&mut self, index: i64, id: Option<&str>) -> Result<(), TransformError> {
        let item = self
            .items
            .get_mut(&index)
            .ok_or_else(|| invalid("missing item"))?;
        if let Some(id) = id {
            if item.id.as_deref().is_some_and(|v| v != id) {
                return Err(invalid("item ID differs from call association"));
            }
            item.id = Some(id.to_owned());
        }
        Ok(())
    }
    pub(super) fn emit_tool(
        &mut self,
        index: i64,
        args: String,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        let Some(ItemState {
            kind:
                ItemKind::Function {
                    call_id,
                    name,
                    tool_index,
                    sent,
                },
            ..
        }) = self.items.get_mut(&index)
        else {
            return Err(invalid("arguments without function item"));
        };
        let id = self
            .flow
            .resolve_or_allocate(
                IdentityRole::ToolCall,
                SourceIdentity::new(Dialect::OpenAi, Some(call_id.clone()), index as u64),
                &self.policy,
            )
            .map_err(|e| TransformError::invalid_result("identity.call", e.to_string()))?
            .emitted_id;
        let mut function = cs::DeltaFunctionCall::builder().arguments(args).build();
        if !*sent {
            function.name = Some(Some(name.clone()));
        }
        let mut call = cs::DeltaToolCall::builder(*tool_index)
            .function(function)
            .build();
        if !*sent {
            call.id = Some(Some(id));
            call.type_ = Some(Some(cs::DeltaToolCallType::Function));
        }
        *sent = true;
        self.emit_delta(cs::Delta::builder().tool_calls(vec![call]).build(), out)
    }
    pub(super) fn flush_text(
        &mut self,
        out: &mut Vec<cs::ChatCompletionChunk>,
    ) -> Result<(), TransformError> {
        loop {
            let Some(item) = self.items.get_mut(&self.content_cursor) else {
                return Ok(());
            };
            let ItemKind::Message {
                parts,
                cursor,
                done,
            } = &mut item.kind
            else {
                self.content_cursor += 1;
                continue;
            };
            let Some(part) = parts.get_mut(cursor) else {
                if *done {
                    self.content_cursor += 1;
                    continue;
                }
                return Ok(());
            };
            let emit = !part.pending.is_empty() || !part.emitted;
            let delta = if emit {
                part.emitted = true;
                let text = std::mem::take(&mut part.pending);
                Some(if part.refusal {
                    cs::Delta::builder().refusal(text).build()
                } else {
                    cs::Delta::builder().content(text).build()
                })
            } else {
                None
            };
            let done = part.done;
            if done {
                *cursor += 1;
            }
            if let Some(delta) = delta {
                self.emit_delta(delta, out)?;
            }
            if !done {
                return Ok(());
            }
        }
    }
}
