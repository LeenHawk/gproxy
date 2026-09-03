use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{Correlation, merge, native};

pub(crate) fn response_blocks(
    content: gemini::Content,
) -> Result<Vec<claude::ContentBlock>, TransformError> {
    let mut correlation = Correlation::default();
    let mut output = Vec::new();
    for part in content.parts {
        if matches!(part.data, Some(gemini::PartData::FunctionCall { .. }))
            && let Some(signature) = part.thought_signature.clone()
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
    claude::ResponseContentBlock::RedactedThinking(claude::RedactedThinkingBlock {
        data: signature,
        type_: claude::RedactedThinkingBlockType::RedactedThinking,
        rest: Default::default(),
    })
}

pub(crate) fn response_part(
    part: gemini::Part,
    correlation: &mut Correlation,
) -> Result<Option<claude::ContentBlock>, TransformError> {
    let gemini::Part {
        thought,
        thought_signature: signature,
        part_metadata,
        media_resolution,
        data,
        metadata,
        rest: mut part_rest,
    } = part;
    if let Some(value) = part_metadata {
        part_rest.insert("partMetadata".into(), serde_json::Value::Object(value));
    }
    if let Some(value) = media_resolution {
        part_rest.insert("mediaResolution".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = metadata {
        part_rest.insert("metadata".into(), serde_json::to_value(value)?);
    }
    let Some(data) = data else {
        return Ok(None);
    };
    Ok(Some(match data {
        gemini::PartData::Text { text, rest } if thought == Some(true) => {
            claude::ResponseContentBlock::Thinking(claude::ThinkingBlock {
                signature,
                thinking: text,
                type_: claude::ThinkingBlockType::Thinking,
                rest: merge(rest, part_rest),
            })
        }
        gemini::PartData::Text { text, rest } => {
            claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                citations: None,
                text,
                type_: claude::TextBlockType::Text,
                rest: merge(rest, part_rest),
            })
        }
        gemini::PartData::FunctionCall {
            function_call,
            rest,
        } => {
            let input = function_call
                .args
                .ok_or_else(|| TransformError::shape("Gemini function call", "args is missing"))?;
            let id = correlation.function_call(function_call.id, &function_call.name);
            claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
                id,
                input,
                name: function_call.name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: super::caller(signature),
                rest: merge(function_call.rest, merge(rest, part_rest)),
            })
        }
        gemini::PartData::ExecutableCode {
            executable_code,
            rest,
        } => native::response_call(
            executable_code,
            correlation,
            merge(rest, part_rest),
            signature,
        )?,
        gemini::PartData::CodeExecutionResult {
            code_execution_result,
            rest,
        } => native::response_result(code_execution_result, correlation, merge(rest, part_rest))?,
        gemini::PartData::ToolCall { tool_call, rest } => {
            let name = crate::models::common::wire_string(&tool_call.tool_type)?;
            let input = tool_call.args.ok_or_else(|| {
                TransformError::shape("Gemini server tool call", "args is missing")
            })?;
            let id = correlation.function_call(tool_call.id, &name);
            claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
                id,
                input,
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest: merge(tool_call.rest, merge(rest, part_rest)),
            })
        }
        gemini::PartData::Raw(_)
        | gemini::PartData::InlineData { .. }
        | gemini::PartData::FunctionResponse { .. }
        | gemini::PartData::FileData { .. }
        | gemini::PartData::ToolResponse { .. } => return Ok(None),
        _future => return Ok(None),
    }))
}
