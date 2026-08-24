use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;

use super::super::claude_to_openai::{Output, State};
use super::super::claude_to_responses::{function_item, reasoning_item};
use super::super::state::merge;
use super::Block;

impl State {
    pub(in crate::common::stream) fn block_start(
        &mut self,
        index: u64,
        block: claude::ContentBlock,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        if !self.started || self.blocks.contains_key(&index) {
            return Err(TransformError::shape(
                "Claude stream",
                "invalid block start",
            ));
        }
        let (state, output) = match block {
            claude::ResponseContentBlock::Text(block) => {
                let id = format!(
                    "msg_{}_{}",
                    self.id.as_deref().expect("started message has an id"),
                    index
                );
                let text = block.text;
                let block_rest = merge(block.rest.clone(), rest.clone());
                let output = match self.output {
                    Output::Chat => (!text.is_empty())
                        .then(|| self.chat_text(text.clone(), block_rest.clone()))
                        .transpose()?
                        .into_iter()
                        .collect(),
                    Output::Responses => vec![
                        self.response_output_item_added(
                            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                                openai::ResponseOutputMessageItem {
                                    type_: openai::ResponseMessageItemType::Message,
                                    id: Some(id.clone()),
                                    role: openai::ResponseOutputMessageRole::Assistant,
                                    content: Vec::new(),
                                    status: openai::ResponseItemLifecycleStatus::InProgress,
                                    phase: None,
                                    rest: block.rest.clone(),
                                },
                            )),
                            index as u32,
                            rest.clone(),
                        )?,
                        self.response_content_part_added(
                            id.clone(),
                            index as u32,
                            openai::ResponseContentPart::OutputText(openai::ResponseOutputText {
                                type_: openai::ResponseOutputTextType::OutputText,
                                annotations: Vec::new(),
                                logprobs: None,
                                text: String::new(),
                                rest: block.rest,
                            }),
                            rest.clone(),
                        )?,
                    ],
                };
                (
                    Block::Text {
                        id,
                        text,
                        rest: block_rest,
                    },
                    output,
                )
            }
            claude::ResponseContentBlock::Thinking(block) => {
                let id = format!(
                    "rs_{}_{}",
                    self.id.as_deref().expect("started message has an id"),
                    index
                );
                let text = block.thinking;
                let block_rest = merge(block.rest.clone(), rest.clone());
                let output = match self.output {
                    Output::Chat => (!text.is_empty())
                        .then(|| self.chat_reasoning(text.clone(), block_rest.clone()))
                        .transpose()?
                        .into_iter()
                        .collect(),
                    Output::Responses => vec![self.response_output_item_added(
                        reasoning_item(
                            id.clone(),
                            text.clone(),
                            block.signature.clone(),
                            block.rest,
                            openai::ResponseItemLifecycleStatus::InProgress,
                        ),
                        index as u32,
                        rest.clone(),
                    )?],
                };
                (
                    Block::Thinking {
                        id,
                        text,
                        signature: block.signature,
                        rest: block_rest,
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
                let block_rest = merge(block.rest.clone(), rest.clone());
                let output = match self.output {
                    Output::Chat => vec![self.chat_tool_start(
                        index as u32,
                        block.id.clone(),
                        block.name.clone(),
                        arguments.clone(),
                        block_rest.clone(),
                    )?],
                    Output::Responses if items::is_buffered_native(&block.name) => Vec::new(),
                    Output::Responses => vec![self.response_output_item_added(
                        function_item(
                            block.id.clone(),
                            block.name.clone(),
                            arguments.clone(),
                            block.rest,
                            openai::ResponseItemLifecycleStatus::InProgress,
                        ),
                        index as u32,
                        rest.clone(),
                    )?],
                };
                (
                    Block::Tool {
                        id: block.id,
                        name: block.name,
                        arguments,
                        rest: block_rest,
                    },
                    output,
                )
            }
            raw => {
                return Err(TransformError::unsupported(
                    "Claude stream block",
                    serde_json::to_string(&raw)?,
                ));
            }
        };
        self.blocks.insert(index, state);
        Ok(output)
    }
}
