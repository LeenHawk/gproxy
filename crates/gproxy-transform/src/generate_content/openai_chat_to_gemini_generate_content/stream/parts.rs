use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::empty_delta;

pub(super) fn convert(
    content: Option<gemini::Content>,
    candidate: u32,
    tools: &mut super::tools::State,
) -> Result<(openai::ChatDelta, bool), TransformError> {
    let Some(content) = content else {
        return Ok((empty_delta(None), false));
    };
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut calls = Vec::new();
    for part in content.parts {
        let thought = part.thought == Some(true);
        match part.data {
            Some(gemini::PartData::Text { text: value, .. }) if thought => reasoning.push(value),
            Some(gemini::PartData::Text { text: value, .. }) => text.push(value),
            Some(gemini::PartData::FunctionCall { function_call, .. }) => {
                calls.push(tools.function(candidate, function_call)?);
            }
            Some(gemini::PartData::ExecutableCode {
                executable_code, ..
            }) => {
                calls.push(tools.code(candidate, executable_code)?);
            }
            Some(gemini::PartData::CodeExecutionResult {
                code_execution_result,
                ..
            }) => tools.result(candidate, code_execution_result)?,
            Some(gemini::PartData::InlineData { inline_data, .. })
                if inline_data.mime_type.starts_with("audio/") =>
            {
                continue;
            }
            Some(gemini::PartData::InlineData { .. })
            | Some(gemini::PartData::FileData { .. })
            | Some(gemini::PartData::Raw(_)) => {}
            Some(gemini::PartData::FunctionResponse { .. })
            | Some(gemini::PartData::ToolCall { .. })
            | Some(gemini::PartData::ToolResponse { .. }) => {}
            Some(other) => {
                return Err(TransformError::unsupported(
                    "Gemini stream part",
                    serde_json::to_string(&other)?,
                ));
            }
            None => {}
        }
    }
    let has_tool = !calls.is_empty();
    Ok((
        openai::ChatDelta {
            role: None,
            content: (!text.is_empty()).then(|| text.join("")),
            reasoning_content: (!reasoning.is_empty()).then(|| reasoning.join("")),
            refusal: None,
            tool_calls: has_tool.then_some(calls),
            function_call: None,
            obfuscation: None,
            rest: Default::default(),
        },
        has_tool,
    ))
}
