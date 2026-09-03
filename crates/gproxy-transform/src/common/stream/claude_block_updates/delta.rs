use bytes::Bytes;
use gproxy_protocol::claude;

use crate::TransformError;
use crate::common::native::items;

use super::super::claude_to_openai::{Output, State};
use super::super::claude_to_responses::ResponseDelta;
use super::{Block, Emission};

impl State {
    pub(in crate::common::stream) fn block_delta(
        &mut self,
        index: u64,
        delta: claude::EventDelta,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut block = self
            .blocks
            .remove(&index)
            .ok_or_else(|| TransformError::shape("Claude stream", "delta before block start"))?;
        let emission = match delta {
            claude::EventDelta::Known(delta) => match (*delta, &mut block, self.output) {
                (
                    claude::KnownEventDelta::Text { text, .. },
                    Block::Text { text: total, id },
                    output,
                ) => {
                    total.push_str(&text);
                    match output {
                        Output::Chat => Emission::ChatText(text),
                        Output::Responses => {
                            Emission::Responses(ResponseDelta::OutputText, id.clone(), text)
                        }
                    }
                }
                (
                    claude::KnownEventDelta::Thinking { thinking, .. },
                    Block::Thinking { text, id, .. },
                    output,
                ) => {
                    text.push_str(&thinking);
                    match output {
                        Output::Chat => Emission::ChatReasoning(thinking),
                        Output::Responses => {
                            Emission::Responses(ResponseDelta::ReasoningText, id.clone(), thinking)
                        }
                    }
                }
                (
                    claude::KnownEventDelta::Signature {
                        signature: delta, ..
                    },
                    Block::Thinking { signature, .. },
                    _,
                ) => {
                    signature.get_or_insert_default().push_str(&delta);
                    Emission::None
                }
                (
                    claude::KnownEventDelta::InputJson { partial_json, .. },
                    Block::Tool {
                        arguments,
                        id,
                        name,
                        ..
                    },
                    output,
                ) => {
                    arguments.push_str(&partial_json);
                    match output {
                        Output::Chat => Emission::ChatTool(partial_json),
                        Output::Responses if items::is_buffered_native(name) => Emission::None,
                        Output::Responses => Emission::Responses(
                            ResponseDelta::FunctionArguments,
                            id.clone(),
                            partial_json,
                        ),
                    }
                }
                (claude::KnownEventDelta::Citations { .. }, _, _) => Emission::None,
                (_, Block::Ignored, _) => Emission::None,
                (
                    other @ (claude::KnownEventDelta::Text { .. }
                    | claude::KnownEventDelta::InputJson { .. }
                    | claude::KnownEventDelta::Thinking { .. }
                    | claude::KnownEventDelta::Signature { .. }
                    | claude::KnownEventDelta::Compaction { .. }),
                    _,
                    _,
                ) => {
                    return Err(TransformError::unsupported(
                        "Claude stream delta",
                        serde_json::to_string(&other)?,
                    ));
                }
            },
            claude::EventDelta::Unknown(_) => Emission::None,
        };
        self.blocks.insert(index, block);
        Ok(match emission {
            Emission::ChatText(text) => vec![self.chat_text(text)?],
            Emission::ChatReasoning(text) => {
                vec![self.chat_reasoning(text)?]
            }
            Emission::ChatTool(arguments) => {
                vec![self.chat_tool_delta(index as u32, arguments)?]
            }
            Emission::Responses(type_, id, delta) => {
                vec![self.response_delta(type_, id, index, delta)?]
            }
            Emission::None => Vec::new(),
        })
    }
}
