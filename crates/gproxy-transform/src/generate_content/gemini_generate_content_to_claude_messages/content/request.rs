use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{Correlation, functions, media, merge, native, request_meta};

pub(crate) fn request_messages(
    contents: Vec<gemini::Content>,
) -> Result<Vec<claude::MessageParam>, TransformError> {
    let mut correlation = Correlation::default();
    let mut output = Vec::new();
    for content in contents {
        if !content.rest.is_empty() {
            return Err(TransformError::unsupported(
                "Gemini content",
                "content rest",
            ));
        }
        let role = request_meta::role(content.role)?;
        let mut blocks = Vec::new();
        for part in content.parts {
            blocks.push(part_to_block(part, &mut correlation)?);
        }
        if !blocks.is_empty() {
            output.push(claude::MessageParam {
                role,
                content: claude::StringOrArray::Array(blocks),
                rest: content.rest,
            });
        }
    }
    Ok(output)
}

pub(super) fn part_to_block(
    part: gemini::Part,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    if part.part_metadata.is_some()
        || part.media_resolution.is_some()
        || part.metadata.is_some()
        || !part.rest.is_empty()
        || part.thought == Some(false)
    {
        return Err(TransformError::unsupported("Gemini part", "part metadata"));
    }
    let gemini::Part {
        thought,
        thought_signature: signature,
        part_metadata: _,
        media_resolution: _,
        data,
        metadata: _,
        rest,
    } = part;
    let allows_thought = matches!(&data, Some(gemini::PartData::Text { .. }));
    let allows_signature = matches!(
        &data,
        Some(
            gemini::PartData::Text { .. }
                | gemini::PartData::FunctionCall { .. }
                | gemini::PartData::ExecutableCode { .. }
        )
    );
    if (thought.is_some() && !allows_thought) || (signature.is_some() && !allows_signature) {
        return Err(TransformError::unsupported(
            "Gemini part",
            "thought fields on incompatible data",
        ));
    }
    Ok(
        match data.ok_or_else(|| TransformError::shape("Gemini part", "part data is missing"))? {
            gemini::PartData::Text { text, rest: data } if thought == Some(true) => {
                reject_data_rest(&data)?;
                claude::ContentBlockParam::Thinking(claude::ThinkingBlock {
                    signature,
                    thinking: text,
                    type_: claude::ThinkingBlockType::Thinking,
                    rest: merge(data, rest),
                })
            }
            gemini::PartData::Text { text, rest: data } => {
                reject_data_rest(&data)?;
                if signature.is_some() {
                    return Err(TransformError::unsupported(
                        "Gemini text part",
                        "thought signature without thought",
                    ));
                }
                functions::text_block(text, merge(data, rest))
            }
            gemini::PartData::InlineData {
                inline_data,
                rest: data,
            } => {
                reject_data_rest(&data)?;
                media::inline(inline_data)?
            }
            gemini::PartData::FileData {
                file_data,
                rest: data,
            } => {
                reject_data_rest(&data)?;
                media::file(file_data)?
            }
            gemini::PartData::FunctionCall {
                function_call,
                rest: data,
            } => functions::function_call_block(
                function_call,
                signature,
                merge(data, rest),
                correlation,
            )?,
            gemini::PartData::FunctionResponse {
                function_response,
                rest: data,
            } => {
                functions::function_result_block(function_response, merge(data, rest), correlation)?
            }
            gemini::PartData::ExecutableCode {
                executable_code,
                rest: data,
            } => native::request_call(executable_code, correlation, merge(data, rest), signature)?,
            gemini::PartData::CodeExecutionResult {
                code_execution_result,
                rest: data,
            } => native::request_result(code_execution_result, correlation, merge(data, rest))?,
            gemini::PartData::ToolCall {
                tool_call,
                rest: data,
            } => functions::server_call_block(tool_call, merge(data, rest), correlation)?,
            gemini::PartData::ToolResponse {
                tool_response,
                rest: data,
            } => functions::server_result_block(tool_response, merge(data, rest), correlation)?,
            gemini::PartData::Raw(raw) => {
                return Err(TransformError::unsupported(
                    "Gemini raw part",
                    raw.to_string(),
                ));
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Gemini part",
                    "future data variant",
                ));
            }
        },
    )
}

fn reject_data_rest(rest: &claude::JsonObject) -> Result<(), TransformError> {
    if !rest.is_empty() {
        return Err(TransformError::unsupported(
            "Gemini part data",
            "rest fields",
        ));
    }
    Ok(())
}
