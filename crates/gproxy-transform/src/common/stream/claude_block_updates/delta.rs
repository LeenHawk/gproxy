use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;

use super::super::claude_to_openai::{Output, State};
use super::super::claude_to_responses::ResponseDelta;
use super::super::state::merge;
use super::{Block, Emission};

impl State {
    pub(in crate::common::stream) fn block_delta(
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
                (claude::KnownEventDelta::Citations { .. }, _, _) => Emission::None,
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
}
