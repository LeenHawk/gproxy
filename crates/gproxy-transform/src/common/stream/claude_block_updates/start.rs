use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;

use super::super::claude_to_openai::{Output, OutputEvent, State};
use super::super::claude_to_responses::{function_item, reasoning_item};
use super::Block;

impl State {
    pub(in crate::common::stream) fn block_start(
        &mut self,
        index: u64,
        block: claude::ContentBlock,
    ) -> Result<Vec<OutputEvent>, TransformError> {
        if !self.started || self.blocks.contains_key(&index) {
            return Err(TransformError::shape(
                "Claude stream",
                "invalid block start",
            ));
        }
        if let Some(model) = crate::common::content::claude_fallback_model(&block) {
            self.model = Some(crate::models::common::wire_string(&model)?.into());
        }
        let (state, output) = match block {
            claude::ResponseContentBlock::Text(block) => {
                let id = format!(
                    "msg_{}_{}",
                    self.id.as_deref().expect("started message has an id"),
                    index
                );
                let text = block.text;
                let output = match self.output {
                    Output::Chat => (!text.is_empty())
                        .then(|| self.chat_text(text.clone()))
                        .transpose()?
                        .into_iter()
                        .collect(),
                    Output::Responses => vec![
                        self.response_output_item_added(
                            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                                crate::wire!(openai::ResponseOutputMessageItem {
                                    type_: openai::ResponseMessageItemType::Message,
                                    id: id.clone(),
                                    role: openai::ResponseOutputMessageRole::Assistant,
                                    content: Vec::new(),
                                    status: openai::ResponseItemLifecycleStatus::InProgress,
                                    phase: None,
                                    rest: Default::default(),
                                }),
                            )),
                            index as u32,
                        )?,
                        self.response_content_part_added(
                            id.clone(),
                            index as u32,
                            openai::ResponseContentPart::OutputText(crate::wire!(
                                openai::ResponseOutputText {
                                    type_: openai::ResponseOutputTextType::OutputText,
                                    annotations: Vec::new(),
                                    logprobs: None,
                                    text: String::new(),
                                    rest: Default::default(),
                                }
                            )),
                        )?,
                    ],
                };
                (Block::Text { id, text }, output)
            }
            claude::ResponseContentBlock::Thinking(block) => {
                let id = format!(
                    "rs_{}_{}",
                    self.id.as_deref().expect("started message has an id"),
                    index
                );
                let text = block.thinking;
                let output = match self.output {
                    Output::Chat => (!text.is_empty())
                        .then(|| self.chat_reasoning(text.clone()))
                        .transpose()?
                        .into_iter()
                        .collect(),
                    Output::Responses => vec![self.response_output_item_added(
                        reasoning_item(
                            id.clone(),
                            text.clone(),
                            block.signature.clone(),
                            openai::ResponseItemLifecycleStatus::InProgress,
                        ),
                        index as u32,
                    )?],
                };
                (
                    Block::Thinking {
                        id,
                        text,
                        signature: block.signature,
                    },
                    output,
                )
            }
            claude::ResponseContentBlock::ToolUse(block) => {
                let arguments = if block.input.is_empty() {
                    String::new()
                } else {
                    serde_json::to_string(&block.input)?
                };
                let output = match self.output {
                    Output::Chat => vec![self.chat_tool_start(
                        index as u32,
                        block.id.clone(),
                        block.name.clone(),
                        arguments.clone(),
                    )?],
                    Output::Responses if items::is_buffered_native(&block.name) => Vec::new(),
                    Output::Responses => vec![self.response_output_item_added(
                        function_item(
                            block.id.clone(),
                            block.name.clone(),
                            arguments.clone(),
                            openai::ResponseItemLifecycleStatus::InProgress,
                        ),
                        index as u32,
                    )?],
                };
                (
                    Block::Tool {
                        id: block.id,
                        name: block.name,
                        arguments,
                    },
                    output,
                )
            }
            claude::ResponseContentBlock::RedactedThinking(_)
            | claude::ResponseContentBlock::ServerToolUse(_)
            | claude::ResponseContentBlock::WebSearchToolResult(_)
            | claude::ResponseContentBlock::WebFetchToolResult(_)
            | claude::ResponseContentBlock::AdvisorToolResult(_)
            | claude::ResponseContentBlock::CodeExecutionToolResult(_)
            | claude::ResponseContentBlock::BashCodeExecutionToolResult(_)
            | claude::ResponseContentBlock::TextEditorCodeExecutionToolResult(_)
            | claude::ResponseContentBlock::ToolSearchToolResult(_)
            | claude::ResponseContentBlock::McpToolUse(_)
            | claude::ResponseContentBlock::McpToolResult(_)
            | claude::ResponseContentBlock::ContainerUpload(_)
            | claude::ResponseContentBlock::Compaction(_)
            | claude::ResponseContentBlock::Fallback(_)
            | claude::ResponseContentBlock::Raw(_) => (Block::Ignored, Vec::new()),
            future => {
                return Err(TransformError::unsupported(
                    "Claude stream block",
                    serde_json::to_string(&future)?,
                ));
            }
        };
        self.blocks.insert(index, state);
        Ok(output)
    }
}
