use crate::claude::create_message::response::ClaudeCreateMessageResponse;
use crate::claude::create_message::stream::{
    BetaMessageDeltaUsage, BetaRawContentBlockDelta, BetaRawMessageDelta, ClaudeStreamEvent,
};
use crate::claude::create_message::types::BetaContentBlock;
use crate::transform::utils::TransformError;

fn stream_start_content_block(content_block: &BetaContentBlock) -> BetaContentBlock {
    match content_block {
        BetaContentBlock::Text(block) => {
            BetaContentBlock::Text(crate::claude::create_message::types::BetaTextBlock {
                citations: None,
                text: String::new(),
                type_: block.type_.clone(),
            })
        }
        BetaContentBlock::Thinking(block) => {
            BetaContentBlock::Thinking(crate::claude::create_message::types::BetaThinkingBlock {
                signature: block.signature.clone(),
                thinking: String::new(),
                type_: block.type_.clone(),
            })
        }
        BetaContentBlock::ToolUse(block) => {
            BetaContentBlock::ToolUse(crate::claude::create_message::types::BetaToolUseBlock {
                id: block.id.clone(),
                input: Default::default(),
                name: block.name.clone(),
                type_: block.type_.clone(),
                cache_control: block.cache_control.clone(),
                caller: block.caller.clone(),
            })
        }
        BetaContentBlock::Compaction(block) => BetaContentBlock::Compaction(
            crate::claude::create_message::types::BetaCompactionBlock {
                content: None,
                type_: block.type_.clone(),
                cache_control: block.cache_control.clone(),
            },
        ),
        _ => content_block.clone(),
    }
}

fn push_content_block_delta_events(
    events: &mut Vec<ClaudeStreamEvent>,
    index: u64,
    content_block: &BetaContentBlock,
) {
    match content_block {
        BetaContentBlock::Text(block) => {
            if !block.text.is_empty() {
                events.push(ClaudeStreamEvent::ContentBlockDelta {
                    delta: BetaRawContentBlockDelta::Text {
                        text: block.text.clone(),
                    },
                    index,
                });
            }
            if let Some(citations) = block.citations.as_ref() {
                for citation in citations {
                    events.push(ClaudeStreamEvent::ContentBlockDelta {
                        delta: BetaRawContentBlockDelta::Citations {
                            citation: citation.clone(),
                        },
                        index,
                    });
                }
            }
        }
        BetaContentBlock::Thinking(block) => {
            if !block.thinking.is_empty() {
                events.push(ClaudeStreamEvent::ContentBlockDelta {
                    delta: BetaRawContentBlockDelta::Thinking {
                        thinking: block.thinking.clone(),
                    },
                    index,
                });
            }
            if !block.signature.is_empty() {
                events.push(ClaudeStreamEvent::ContentBlockDelta {
                    delta: BetaRawContentBlockDelta::Signature {
                        signature: block.signature.clone(),
                    },
                    index,
                });
            }
        }
        BetaContentBlock::ToolUse(block) => {
            if !block.input.is_empty()
                && let Ok(input_json) = serde_json::to_string(&block.input)
                && !input_json.is_empty()
                && input_json != "{}"
            {
                events.push(ClaudeStreamEvent::ContentBlockDelta {
                    delta: BetaRawContentBlockDelta::InputJson {
                        partial_json: input_json,
                    },
                    index,
                });
            }
        }
        BetaContentBlock::Compaction(block) if block.content.is_some() => {
            events.push(ClaudeStreamEvent::ContentBlockDelta {
                delta: BetaRawContentBlockDelta::Compaction {
                    content: block.content.clone(),
                },
                index,
            });
        }
        _ => {}
    }
}

pub fn nonstream_to_stream(
    value: ClaudeCreateMessageResponse,
    out: &mut Vec<ClaudeStreamEvent>,
) -> Result<(), TransformError> {
    match value {
        ClaudeCreateMessageResponse::Success { body, .. } => {
            let mut start_message = body.clone();
            start_message.content = Vec::new();
            start_message.context_management = None;
            start_message.stop_reason = None;
            start_message.stop_sequence = None;
            start_message.usage.output_tokens = 0;

            out.push(ClaudeStreamEvent::MessageStart {
                message: start_message,
            });

            for (index, content_block) in body.content.iter().enumerate() {
                let index = index as u64;
                out.push(ClaudeStreamEvent::ContentBlockStart {
                    content_block: stream_start_content_block(content_block),
                    index,
                });

                push_content_block_delta_events(out, index, content_block);

                out.push(ClaudeStreamEvent::ContentBlockStop { index });
            }

            out.push(ClaudeStreamEvent::MessageDelta {
                context_management: body.context_management.clone(),
                delta: BetaRawMessageDelta {
                    container: body.container.clone(),
                    stop_reason: body.stop_reason.clone(),
                    stop_sequence: body.stop_sequence.clone(),
                },
                usage: BetaMessageDeltaUsage {
                    cache_creation_input_tokens: Some(body.usage.cache_creation_input_tokens),
                    cache_read_input_tokens: Some(body.usage.cache_read_input_tokens),
                    input_tokens: Some(body.usage.input_tokens),
                    iterations: Some(body.usage.iterations.clone()),
                    output_tokens: body.usage.output_tokens,
                    server_tool_use: Some(body.usage.server_tool_use.clone()),
                },
            });

            out.push(ClaudeStreamEvent::MessageStop {});

            Ok(())
        }
        ClaudeCreateMessageResponse::Error { body, .. } => {
            out.push(ClaudeStreamEvent::Error { error: body.error });
            Ok(())
        }
    }
}
