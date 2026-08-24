use gproxy_channel_api::ChannelError;
use gproxy_protocol::{aws, claude};

pub(super) fn tool_result(
    block: claude::ToolResultBlock,
) -> Result<aws::ToolResultBlock, ChannelError> {
    let content = match block.content {
        None => vec![aws::ToolResultContentBlock::Text {
            text: String::new(),
            rest: Default::default(),
        }],
        Some(claude::ToolResultContent::Text(text)) => vec![aws::ToolResultContentBlock::Text {
            text,
            rest: Default::default(),
        }],
        Some(claude::ToolResultContent::Blocks(blocks)) => blocks
            .into_iter()
            .map(|block| match block {
                claude::ToolResultContentBlock::Text(text) => {
                    Ok(aws::ToolResultContentBlock::Text {
                        text: text.text,
                        rest: text.rest,
                    })
                }
                _ => Err(super::content::prepare(
                    "Bedrock tool result supports text blocks only",
                )),
            })
            .collect::<Result<_, _>>()?,
        Some(claude::ToolResultContent::Raw(_)) => {
            return Err(super::content::prepare("unsupported raw tool result"));
        }
        Some(_) => {
            return Err(super::content::prepare(
                "unsupported Claude tool result variant",
            ));
        }
    };
    Ok(aws::ToolResultBlock {
        tool_use_id: block.tool_use_id,
        content,
        status: Some(aws::ToolResultStatus::Known(
            if block.is_error == Some(true) {
                aws::ToolResultStatusKnown::Error
            } else {
                aws::ToolResultStatusKnown::Success
            },
        )),
        type_: None,
        rest: block.rest,
    })
}
