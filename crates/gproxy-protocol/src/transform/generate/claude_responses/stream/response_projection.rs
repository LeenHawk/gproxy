use super::{
    common::{id, invalid, limit},
    response_items::*,
    responses_to_claude::ResponsesToClaudeStream,
};
use crate::{
    transform::{TransformError, identity::IdentityRole},
    wire::claude::{generate_content as c, stream as cs},
};

impl ResponsesToClaudeStream {
    pub(super) fn flush_items(
        &mut self,
        out: &mut Vec<cs::StreamEvent>,
    ) -> Result<(), TransformError> {
        loop {
            let index = self.cursor;
            let Some(mut item) = self.items.remove(&index) else {
                return Ok(());
            };
            let advance = match &mut item.kind {
                Kind::Excluded => item.done,
                Kind::Message { parts } => {
                    for part in parts.values_mut() {
                        if part.block.is_none() {
                            let block = self.allocate_block()?;
                            part.block = Some(block);
                            self.emit(
                                out,
                                cs::StreamEvent::ContentBlockStart(
                                    cs::ContentBlockStartEvent::builder(
                                        block,
                                        c::ResponseContentBlock::Text(
                                            c::ResponseTextBlock::builder(
                                                c::ResponseTextBlockType::Tag,
                                                String::new(),
                                            )
                                            .build(),
                                        ),
                                    )
                                    .build(),
                                ),
                            )?;
                            let text = std::mem::take(&mut part.pending);
                            self.held_bytes -= text.len();
                            item.held -= text.len();
                            if !text.is_empty() {
                                self.emit(
                                    out,
                                    cs::StreamEvent::ContentBlockDelta(
                                        cs::ContentBlockDeltaEvent::builder(
                                            block,
                                            cs::ContentBlockDelta::Text(
                                                cs::TextDelta::builder(text).build(),
                                            ),
                                        )
                                        .build(),
                                    ),
                                )?;
                            }
                        }
                        if part.done && !part.closed {
                            part.closed = true;
                            self.emit(
                                out,
                                cs::StreamEvent::ContentBlockStop(
                                    cs::ContentBlockStopEvent::builder(part.block.unwrap()).build(),
                                ),
                            )?;
                        }
                    }
                    item.done
                }
                Kind::Function {
                    call_id,
                    name,
                    caller,
                    pending,
                    block,
                    arguments_done,
                    closed,
                } => {
                    if block.is_none() {
                        let n = self.allocate_block()?;
                        *block = Some(n);
                        let id = id(
                            &mut self.flow,
                            &self.policy,
                            crate::Dialect::OpenAi,
                            IdentityRole::ToolCall,
                            IdentityRole::ToolCall,
                            Some(call_id.clone()),
                            index as u64,
                        )?;
                        let mut tool = c::ResponseToolUseBlock::builder(
                            c::ResponseToolUseBlockType::Tag,
                            id,
                            Default::default(),
                            name.clone(),
                        )
                        .build();
                        tool.caller = caller.clone();
                        self.emit(
                            out,
                            cs::StreamEvent::ContentBlockStart(
                                cs::ContentBlockStartEvent::builder(
                                    n,
                                    c::ResponseContentBlock::ToolUse(tool),
                                )
                                .build(),
                            ),
                        )?;
                        let args = std::mem::take(pending);
                        self.held_bytes -= args.len();
                        item.held -= args.len();
                        if !args.is_empty() {
                            self.emit(
                                out,
                                cs::StreamEvent::ContentBlockDelta(
                                    cs::ContentBlockDeltaEvent::builder(
                                        n,
                                        cs::ContentBlockDelta::InputJson(
                                            cs::InputJsonDelta::builder(args).build(),
                                        ),
                                    )
                                    .build(),
                                ),
                            )?;
                        }
                    }
                    if *arguments_done && !*closed {
                        *closed = true;
                        self.emit(
                            out,
                            cs::StreamEvent::ContentBlockStop(
                                cs::ContentBlockStopEvent::builder(block.unwrap()).build(),
                            ),
                        )?;
                    }
                    true
                }
                Kind::Reasoning => true,
                Kind::Mcp {
                    final_item,
                    projected,
                } => {
                    if item.done {
                        if !*projected {
                            let call = final_item
                                .take()
                                .ok_or_else(|| invalid("missing completed MCP call"))?;
                            self.held_bytes -= item.held;
                            item.held = 0;
                            let blocks = super::super::response::mcp::to_claude(
                                call,
                                index as u64,
                                &mut self.flow,
                                &self.policy,
                                &mut self.report,
                            )?;
                            for block in blocks {
                                self.complete_block(out, block)?;
                            }
                            *projected = true;
                        }
                        true
                    } else {
                        false
                    }
                }
            };
            self.items.insert(index, item);
            if !advance {
                return Ok(());
            }
            self.cursor = self.cursor.checked_add(1).ok_or_else(limit)?;
        }
    }
}
