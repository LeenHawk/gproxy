use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;

use super::claude_to_openai::{Output, State, empty_chat_delta};
use super::claude_to_responses::{ResponseDelta, function_item, reasoning_item};
use super::state::merge;

pub(super) enum Block {
    Text {
        id: String,
        text: String,
        rest: openai::Rest,
    },
    Thinking {
        id: String,
        text: String,
        signature: Option<String>,
        rest: openai::Rest,
    },
    Tool {
        id: String,
        name: String,
        arguments: String,
        rest: openai::Rest,
    },
}

enum Emission {
    ChatText(String, openai::Rest),
    ChatReasoning(String, openai::Rest),
    ChatTool(String, openai::Rest),
    Responses(ResponseDelta, String, String, openai::Rest),
    None,
}

impl State {
    pub(super) fn block_start(
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

    pub(super) fn block_delta(
        &mut self,
        index: u64,
        delta: claude::EventDelta,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut block = self
            .blocks
            .remove(&index)
            .ok_or_else(|| TransformError::shape("Claude stream", "delta before block start"))?;
        let emission = match delta {
            claude::EventDelta::Known(delta) => match (*delta, &mut block, self.output) {
                (
                    claude::KnownEventDelta::Text {
                        text,
                        rest: delta_rest,
                    },
                    Block::Text {
                        text: total,
                        id,
                        rest: block_rest,
                    },
                    output,
                ) => {
                    total.push_str(&text);
                    let event_rest = merge(delta_rest, rest.clone());
                    block_rest.extend(event_rest.clone());
                    match output {
                        Output::Chat => Emission::ChatText(text, event_rest),
                        Output::Responses => Emission::Responses(
                            ResponseDelta::OutputText,
                            id.clone(),
                            text,
                            event_rest,
                        ),
                    }
                }
                (
                    claude::KnownEventDelta::Thinking {
                        thinking,
                        rest: delta_rest,
                        ..
                    },
                    Block::Thinking {
                        text,
                        id,
                        rest: block_rest,
                        ..
                    },
                    output,
                ) => {
                    text.push_str(&thinking);
                    let event_rest = merge(delta_rest, rest.clone());
                    block_rest.extend(event_rest.clone());
                    match output {
                        Output::Chat => Emission::ChatReasoning(thinking, event_rest),
                        Output::Responses => Emission::Responses(
                            ResponseDelta::ReasoningText,
                            id.clone(),
                            thinking,
                            event_rest,
                        ),
                    }
                }
                (
                    claude::KnownEventDelta::Signature {
                        signature: delta,
                        rest: delta_rest,
                    },
                    Block::Thinking {
                        signature,
                        rest: block_rest,
                        ..
                    },
                    _,
                ) => {
                    signature.get_or_insert_default().push_str(&delta);
                    block_rest.extend(merge(delta_rest, rest.clone()));
                    Emission::None
                }
                (
                    claude::KnownEventDelta::InputJson {
                        partial_json,
                        rest: delta_rest,
                    },
                    Block::Tool {
                        arguments,
                        id,
                        name,
                        rest: block_rest,
                        ..
                    },
                    output,
                ) => {
                    arguments.push_str(&partial_json);
                    let event_rest = merge(delta_rest, rest.clone());
                    block_rest.extend(event_rest.clone());
                    match output {
                        Output::Chat => Emission::ChatTool(partial_json, event_rest),
                        Output::Responses if items::is_buffered_native(name) => Emission::None,
                        Output::Responses => Emission::Responses(
                            ResponseDelta::FunctionArguments,
                            id.clone(),
                            partial_json,
                            event_rest,
                        ),
                    }
                }
                (other, _, _) => {
                    return Err(TransformError::unsupported(
                        "Claude stream delta",
                        serde_json::to_string(&other)?,
                    ));
                }
            },
            claude::EventDelta::Unknown(raw) => {
                return Err(TransformError::unsupported(
                    "Claude stream delta",
                    raw.to_string(),
                ));
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Claude stream delta",
                    "future delta variant",
                ));
            }
        };
        self.blocks.insert(index, block);
        Ok(match emission {
            Emission::ChatText(text, rest) => vec![self.chat_text(text, rest)?],
            Emission::ChatReasoning(text, rest) => vec![self.chat_reasoning(text, rest)?],
            Emission::ChatTool(arguments, rest) => {
                vec![self.chat_tool_delta(index as u32, arguments, rest)?]
            }
            Emission::Responses(type_, id, delta, rest) => {
                vec![self.response_delta(type_, id, index, delta, rest)?]
            }
            Emission::None => Vec::new(),
        })
    }

    pub(super) fn block_stop(
        &mut self,
        index: u64,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let block = self
            .blocks
            .remove(&index)
            .ok_or_else(|| TransformError::shape("Claude stream", "stop before block start"))?;
        if matches!(self.output, Output::Chat) {
            return (!rest.is_empty())
                .then(|| self.chat_chunk(empty_chat_delta(rest), None, None))
                .transpose()
                .map(|frame| frame.into_iter().collect());
        }
        if let Block::Tool {
            id,
            name,
            arguments,
            rest: block_rest,
        } = &block
            && items::is_buffered_native(name)
        {
            let input: claude::JsonObject = if arguments.is_empty() {
                Default::default()
            } else {
                serde_json::from_str(arguments)?
            };
            let completed_rest = merge(block_rest.clone(), rest.clone());
            let (in_progress, _) = items::claude_call(
                id.clone(),
                input.clone(),
                name.clone(),
                block_rest.clone(),
                openai::ResponseItemLifecycleStatus::InProgress,
            )?;
            let (completed, _) = items::claude_call(
                id.clone(),
                input,
                name.clone(),
                completed_rest,
                openai::ResponseItemLifecycleStatus::Completed,
            )?;
            let in_progress = openai::ResponseItem::Typed(Box::new(in_progress));
            let completed = openai::ResponseItem::Typed(Box::new(completed));
            self.completed.push(completed.clone());
            return Ok(vec![
                self.response_output_item_added(in_progress, index as u32, Default::default())?,
                self.response_output_item_done(completed, index as u32, rest)?,
            ]);
        }
        let item = match block {
            Block::Text {
                id,
                text,
                rest: block_rest,
            } => openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                openai::ResponseOutputMessageItem {
                    type_: openai::ResponseMessageItemType::Message,
                    id: Some(id),
                    role: openai::ResponseOutputMessageRole::Assistant,
                    content: vec![openai::ResponseMessageOutputContentPart::OutputText(
                        openai::ResponseOutputText {
                            type_: openai::ResponseOutputTextType::OutputText,
                            annotations: Vec::new(),
                            logprobs: None,
                            text,
                            rest: merge(block_rest, rest.clone()),
                        },
                    )],
                    status: openai::ResponseItemLifecycleStatus::Completed,
                    phase: None,
                    rest: Default::default(),
                },
            )),
            Block::Thinking {
                id,
                text,
                signature,
                rest: block_rest,
            } => reasoning_item(
                id,
                text,
                signature,
                merge(block_rest, rest.clone()),
                openai::ResponseItemLifecycleStatus::Completed,
            ),
            Block::Tool {
                id,
                name,
                arguments,
                rest: block_rest,
            } => function_item(
                id,
                name,
                arguments,
                merge(block_rest, rest.clone()),
                openai::ResponseItemLifecycleStatus::Completed,
            ),
        };
        self.completed.push(item.clone());
        Ok(vec![self.response_output_item_done(
            item,
            index as u32,
            rest,
        )?])
    }
}
