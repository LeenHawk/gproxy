use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{Correlation, functions, media, native, request_meta};

pub(crate) fn request_messages(
    contents: Vec<gemini::Content>,
) -> Result<Vec<claude::MessageParam>, TransformError> {
    let mut correlation = Correlation::default();
    let mut output = Vec::new();
    for content in contents {
        let role = request_meta::role(content.role)?;
        let mut blocks = Vec::new();
        for part in content.parts {
            if let Some(block) = part_to_block(part, &mut correlation)? {
                blocks.push(block);
            }
        }
        if !blocks.is_empty() {
            output.push(claude::MessageParam {
                role,
                content: claude::StringOrArray::Array(blocks),
                rest: Default::default(),
            });
        }
    }
    Ok(output)
}

pub(super) fn part_to_block(
    part: gemini::Part,
    correlation: &mut Correlation,
) -> Result<Option<claude::ContentBlockParam>, TransformError> {
    let gemini::Part {
        thought,
        thought_signature: signature,
        part_metadata: _,
        media_resolution: _,
        data,
        metadata: _,
        rest: _,
    } = part;
    let Some(data) = data else {
        return Ok(None);
    };
    Ok(Some(match data {
        gemini::PartData::Text { text, rest: data } if thought == Some(true) => {
            claude::ContentBlockParam::Thinking(claude::ThinkingBlock {
                signature,
                thinking: text,
                type_: claude::ThinkingBlockType::Thinking,
                rest: data,
            })
        }
        gemini::PartData::Text { text, rest: data } => functions::text_block(text, data),
        gemini::PartData::InlineData {
            inline_data,
            rest: _,
        } => match media::inline(inline_data) {
            Ok(block) => block,
            Err(TransformError::Unsupported { .. }) => return Ok(None),
            Err(error) => return Err(error),
        },
        gemini::PartData::FileData { file_data, rest: _ } => match media::file(file_data) {
            Ok(block) => block,
            Err(TransformError::Unsupported { .. }) => return Ok(None),
            Err(error) => return Err(error),
        },
        gemini::PartData::FunctionCall {
            function_call,
            rest: data,
        } => functions::function_call_block(function_call, signature, data, correlation)?,
        gemini::PartData::FunctionResponse {
            function_response,
            rest: data,
        } => functions::function_result_block(function_response, data, correlation)?,
        gemini::PartData::ExecutableCode {
            executable_code,
            rest: data,
        } => native::request_call(executable_code, correlation, data, signature)?,
        gemini::PartData::CodeExecutionResult {
            code_execution_result,
            rest: data,
        } => native::request_result(code_execution_result, correlation, data)?,
        gemini::PartData::ToolCall {
            tool_call,
            rest: data,
        } => functions::server_call_block(tool_call, data, correlation)?,
        gemini::PartData::ToolResponse {
            tool_response,
            rest: data,
        } => functions::server_result_block(tool_response, data, correlation)?,
        gemini::PartData::Raw(_) => return Ok(None),
        _future => return Ok(None),
    }))
}
