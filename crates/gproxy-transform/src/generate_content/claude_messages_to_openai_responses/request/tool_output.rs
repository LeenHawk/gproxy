use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::responses;

pub(super) fn function_output(
    block: claude::ToolResultBlock,
) -> Result<openai::ResponseItem, TransformError> {
    Ok(openai::ResponseItem::Typed(Box::new(
        openai::TypedResponseItem::FunctionCallOutput {
            call_id: block.tool_use_id,
            output: tool_output(block.content)?,
            id: None,
            caller: None,
            name: None,
            namespace: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: Default::default(),
        },
    )))
}

pub(super) fn reasoning_item(
    block: claude::ThinkingBlock,
) -> Result<openai::ResponseItem, TransformError> {
    let content = (!block.thinking.is_empty()).then(|| {
        vec![openai::ResponseReasoningTextPart {
            type_: openai::ResponseReasoningTextType::ReasoningText,
            text: block.thinking,
            rest: Default::default(),
        }]
    });
    Ok(openai::ResponseItem::Typed(Box::new(
        openai::TypedResponseItem::Reasoning {
            id: None,
            summary: Vec::new(),
            content,
            encrypted_content: block.signature,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            rest: Default::default(),
        },
    )))
}

pub(super) fn redacted_reasoning_item(
    block: claude::RedactedThinkingBlock,
) -> Result<openai::ResponseItem, TransformError> {
    Ok(openai::ResponseItem::Typed(Box::new(
        openai::TypedResponseItem::Reasoning {
            id: None,
            summary: Vec::new(),
            content: None,
            encrypted_content: Some(block.data),
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            rest: Default::default(),
        },
    )))
}

fn tool_output(
    content: Option<claude::ToolResultContent>,
) -> Result<openai::ResponseOutput, TransformError> {
    Ok(match content {
        None => {
            return Err(TransformError::shape(
                "Claude tool result",
                "content is missing",
            ));
        }
        Some(claude::ToolResultContent::Text(text)) => openai::ResponseOutput::Text(text),
        Some(claude::ToolResultContent::Blocks(blocks)) => {
            let mut output = Vec::new();
            for block in blocks {
                let block: claude::ContentBlockParam =
                    serde_json::from_value(serde_json::to_value(block)?)?;
                output.extend(
                    responses::claude_to_input(vec![block])?
                        .into_iter()
                        .map(tool_output_part)
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            openai::ResponseOutput::Parts(output)
        }
        Some(claude::ToolResultContent::Raw(raw)) => serde_json::from_value(raw)?,
        Some(_) => {
            return Err(TransformError::unsupported(
                "Claude tool output",
                "future output shape",
            ));
        }
    })
}

fn tool_output_part(
    part: openai::ResponseInputContentPart,
) -> Result<openai::ResponseToolOutputContentPart, TransformError> {
    Ok(match part {
        openai::ResponseInputContentPart::InputText(part) => {
            openai::ResponseToolOutputContentPart::InputText(part)
        }
        openai::ResponseInputContentPart::InputImage(part) => {
            openai::ResponseToolOutputContentPart::InputImage(part)
        }
        openai::ResponseInputContentPart::InputFile(part) => {
            openai::ResponseToolOutputContentPart::InputFile(part)
        }
        openai::ResponseInputContentPart::InputAudio(_) => {
            return Err(TransformError::unsupported(
                "Claude tool output",
                "audio content",
            ));
        }
    })
}
