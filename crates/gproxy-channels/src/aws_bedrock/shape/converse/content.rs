use gproxy_channel_api::ChannelError;
use gproxy_protocol::{aws, claude};
use serde_json::Value;

pub(super) fn messages(
    messages: Vec<claude::MessageParam>,
) -> Result<Vec<aws::Message>, ChannelError> {
    messages
        .into_iter()
        .map(|message| {
            let role: aws::ConversationRole = transcode(message.role, "message role")?;
            let assistant = serde_json::to_value(&role)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                == Some("assistant".into());
            Ok(aws::Message {
                role,
                content: message_content(message.content, assistant)?,
                rest: message.rest,
            })
        })
        .collect()
}

pub(super) fn system(
    system: Option<claude::SystemPrompt>,
) -> Result<Option<Vec<aws::SystemContentBlock>>, ChannelError> {
    let Some(system) = system else {
        return Ok(None);
    };
    let blocks = match system {
        claude::StringOrArray::String(text) => vec![aws::SystemContentBlock::Text {
            text,
            rest: Default::default(),
        }],
        claude::StringOrArray::Array(blocks) => blocks
            .into_iter()
            .flat_map(|block| {
                let mut output = vec![aws::SystemContentBlock::Text {
                    text: block.text,
                    rest: block.rest,
                }];
                if let Some(cache) = block.cache_control {
                    output.push(aws::SystemContentBlock::CachePoint {
                        cache_point: cache_point(cache),
                        rest: Default::default(),
                    });
                }
                output
            })
            .collect(),
        claude::StringOrArray::Raw(_) => {
            return Err(prepare("unsupported raw Claude system prompt"));
        }
        _ => return Err(prepare("unsupported Claude system prompt variant")),
    };
    Ok(Some(blocks))
}

fn message_content(
    content: claude::MessageContent,
    assistant: bool,
) -> Result<Vec<aws::ContentBlock>, ChannelError> {
    match content {
        claude::StringOrArray::String(text) => Ok(vec![aws::ContentBlock::Text {
            text,
            rest: Default::default(),
        }]),
        claude::StringOrArray::Array(blocks) => blocks
            .into_iter()
            .map(|block| content_block(block, assistant))
            .collect::<Result<Vec<_>, _>>()
            .map(|blocks| blocks.into_iter().flatten().collect()),
        claude::StringOrArray::Raw(_) => Err(prepare("unsupported raw Claude message content")),
        _ => Err(prepare("unsupported Claude message content variant")),
    }
}

fn content_block(
    block: claude::ContentBlockParam,
    assistant: bool,
) -> Result<Vec<aws::ContentBlock>, ChannelError> {
    let (mapped, cache) = match block {
        claude::ContentBlockParam::Text(block) => (
            aws::ContentBlock::Text {
                text: block.text,
                rest: block.rest,
            },
            block.cache_control,
        ),
        claude::ContentBlockParam::Image(block) => (
            aws::ContentBlock::Image {
                image: super::media::image(block.source)?,
                rest: block.rest,
            },
            block.cache_control,
        ),
        claude::ContentBlockParam::Document(block) => (
            aws::ContentBlock::Document {
                document: super::media::document(block.source, block.title, block.context)?,
                rest: block.rest,
            },
            block.cache_control,
        ),
        claude::ContentBlockParam::ToolUse(block) => (
            aws::ContentBlock::ToolUse {
                tool_use: aws::ToolUseBlock {
                    tool_use_id: block.id,
                    name: block.name,
                    input: Value::Object(block.input),
                    type_: None,
                    rest: block.rest,
                },
                rest: Default::default(),
            },
            block.cache_control,
        ),
        claude::ContentBlockParam::ToolResult(block) => (
            aws::ContentBlock::ToolResult {
                tool_result: super::results::tool_result(block)?,
                rest: Default::default(),
            },
            None,
        ),
        claude::ContentBlockParam::Thinking(block) if assistant => (
            aws::ContentBlock::ReasoningContent {
                reasoning_content: aws::ReasoningContentBlock::ReasoningText {
                    reasoning_text: aws::ReasoningTextBlock {
                        text: block.thinking,
                        signature: block.signature,
                        rest: block.rest,
                    },
                    rest: Default::default(),
                },
                rest: Default::default(),
            },
            None,
        ),
        claude::ContentBlockParam::RedactedThinking(block) if assistant => (
            aws::ContentBlock::ReasoningContent {
                reasoning_content: aws::ReasoningContentBlock::RedactedContent {
                    redacted_content: block.data,
                    rest: block.rest,
                },
                rest: Default::default(),
            },
            None,
        ),
        _ => {
            return Err(prepare(
                "Claude content block is unsupported by Bedrock Converse",
            ));
        }
    };
    let mut output = vec![mapped];
    if let Some(cache) = cache {
        output.push(aws::ContentBlock::CachePoint {
            cache_point: cache_point(cache),
            rest: Default::default(),
        });
    }
    Ok(output)
}

pub(super) fn cache_point(cache: claude::CacheControl) -> aws::CachePointBlock {
    aws::CachePointBlock {
        type_: aws::CachePointType::Known(aws::CachePointTypeKnown::Default),
        ttl: cache.ttl.and_then(|ttl| transcode(ttl, "cache ttl").ok()),
        rest: cache.rest,
    }
}

pub(super) fn transcode<T: serde::Serialize, U: serde::de::DeserializeOwned>(
    value: T,
    field: &str,
) -> Result<U, ChannelError> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|error| prepare(format!("{field}: {error}")))
}
pub(super) fn prepare(message: impl Into<String>) -> ChannelError {
    ChannelError::Prepare(message.into())
}
