use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{Correlation, merge, native};

pub(crate) fn response_blocks(
    content: gemini::Content,
) -> Result<Vec<claude::ContentBlock>, TransformError> {
    if !content.rest.is_empty()
        || !matches!(
            content.role,
            None | Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model))
        )
    {
        return Err(TransformError::unsupported(
            "Gemini response content",
            "role or rest fields",
        ));
    }
    let mut correlation = Correlation::default();
    content
        .parts
        .into_iter()
        .map(|part| response_part(part, &mut correlation))
        .collect()
}

pub(crate) fn response_part(
    part: gemini::Part,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlock, TransformError> {
    let raw = serde_json::to_value(&part)?;
    let gemini::Part {
        thought,
        thought_signature: signature,
        part_metadata,
        media_resolution,
        data,
        metadata,
        rest: mut part_rest,
    } = part;
    if thought == Some(false) {
        return Err(TransformError::unsupported(
            "Gemini response part",
            "explicit thought=false",
        ));
    }
    let allows_signature = (thought == Some(true)
        && matches!(&data, Some(gemini::PartData::Text { .. })))
        || matches!(
            &data,
            Some(gemini::PartData::FunctionCall { .. } | gemini::PartData::ExecutableCode { .. })
        );
    if signature.is_some() && !allows_signature {
        return Err(TransformError::unsupported(
            "Gemini response part",
            "thought signature on incompatible data",
        ));
    }
    if let Some(value) = part_metadata {
        part_rest.insert("partMetadata".into(), serde_json::Value::Object(value));
    }
    if let Some(value) = media_resolution {
        part_rest.insert("mediaResolution".into(), serde_json::to_value(value)?);
    }
    if let Some(value) = metadata {
        part_rest.insert("metadata".into(), serde_json::to_value(value)?);
    }
    Ok(
        match data
            .ok_or_else(|| TransformError::shape("Gemini response part", "part data is missing"))?
        {
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
                let input = function_call.args.ok_or_else(|| {
                    TransformError::shape("Gemini function call", "args is missing")
                })?;
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
            } => {
                native::response_result(code_execution_result, correlation, merge(rest, part_rest))?
            }
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
            gemini::PartData::Raw(_) => claude::ResponseContentBlock::Raw(raw),
            other => {
                return Err(TransformError::unsupported(
                    "Gemini response part",
                    serde_json::to_string(&other)?,
                ));
            }
        },
    )
}
