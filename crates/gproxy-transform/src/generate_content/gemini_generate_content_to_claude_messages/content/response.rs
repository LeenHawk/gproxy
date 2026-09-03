use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{Correlation, native};

pub(crate) fn response_blocks(
    content: gemini::Content,
) -> Result<Vec<claude::ContentBlock>, TransformError> {
    let mut correlation = Correlation::default();
    let mut output = Vec::new();
    let mut pending_signature = None;
    for mut part in content.parts {
        if part.data.is_none() && part.thought == Some(true) {
            pending_signature = part.thought_signature.take();
            continue;
        }
        if matches!(part.data, Some(gemini::PartData::FunctionCall { .. }))
            && let Some(signature) = part.thought_signature.take().or(pending_signature.take())
        {
            output.push(signature_block(signature));
        }
        if let Some(block) = response_part(part, &mut correlation)? {
            output.push(block);
        }
    }
    Ok(output)
}

fn signature_block(signature: String) -> claude::ContentBlock {
    claude::ResponseContentBlock::RedactedThinking(crate::wire!(claude::RedactedThinkingBlock {
        data: signature,
        type_: claude::RedactedThinkingBlockType::RedactedThinking,
        rest: Default::default(),
    }))
}

pub(crate) fn response_part(
    part: gemini::Part,
    correlation: &mut Correlation,
) -> Result<Option<claude::ContentBlock>, TransformError> {
    let gemini::Part {
        thought,
        thought_signature: signature,
        part_metadata: _,
        media_resolution: _,
        data,
        ..
    } = part;
    let Some(data) = data else {
        return Ok(None);
    };
    Ok(Some(match data {
        gemini::PartData::Text { text, .. } if thought == Some(true) => {
            claude::ResponseContentBlock::Thinking(crate::wire!(claude::ThinkingBlock {
                signature,
                thinking: text,
                type_: claude::ThinkingBlockType::Thinking,
                rest: Default::default(),
            }))
        }
        gemini::PartData::Text { text, .. } => {
            claude::ResponseContentBlock::Text(crate::wire!(claude::ResponseTextBlock {
                citations: None,
                text,
                type_: claude::TextBlockType::Text,
                rest: Default::default(),
            }))
        }
        gemini::PartData::FunctionCall { function_call, .. } => {
            let input = function_call
                .args
                .ok_or_else(|| TransformError::shape("Gemini function call", "args is missing"))?;
            let id = correlation.function_call(function_call.id, &function_call.name);
            claude::ResponseContentBlock::ToolUse(crate::wire!(claude::ResponseToolUseBlock {
                id,
                input,
                name: function_call.name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest: Default::default(),
            }))
        }
        gemini::PartData::ExecutableCode {
            executable_code, ..
        } => native::response_call(executable_code, correlation)?,
        gemini::PartData::CodeExecutionResult {
            code_execution_result,
            ..
        } => native::response_result(code_execution_result, correlation)?,
        gemini::PartData::ToolCall { tool_call, .. } => {
            let name = crate::models::common::wire_string(&tool_call.tool_type)?;
            let input = tool_call.args.ok_or_else(|| {
                TransformError::shape("Gemini server tool call", "args is missing")
            })?;
            let id = correlation.function_call(tool_call.id, &name);
            claude::ResponseContentBlock::ToolUse(crate::wire!(claude::ResponseToolUseBlock {
                id,
                input,
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest: Default::default(),
            }))
        }
        gemini::PartData::Raw(_)
        | gemini::PartData::InlineData { .. }
        | gemini::PartData::FunctionResponse { .. }
        | gemini::PartData::FileData { .. }
        | gemini::PartData::ToolResponse { .. } => return Ok(None),
        _future => return Ok(None),
    }))
}
