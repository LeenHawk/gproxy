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
    let mut rest = content.rest;
    for part in content.parts {
        let thought = part.thought == Some(true);
        match part.data {
            Some(gemini::PartData::Text { text: value, .. }) if thought => reasoning.push(value),
            Some(gemini::PartData::Text { text: value, .. }) => text.push(value),
            Some(gemini::PartData::FunctionCall {
                function_call,
                rest,
            }) => {
                calls.push(tools.function(candidate, function_call, merge(part.rest, rest))?);
            }
            Some(gemini::PartData::ExecutableCode {
                executable_code,
                rest,
            }) => {
                calls.push(tools.code(candidate, executable_code, merge(part.rest, rest))?);
            }
            Some(gemini::PartData::CodeExecutionResult {
                code_execution_result,
                rest: data_rest,
            }) => calls.push(tools.result(
                candidate,
                code_execution_result,
                merge(part.rest, data_rest),
            )?),
            Some(gemini::PartData::InlineData { inline_data, .. })
                if inline_data.mime_type.starts_with("audio/") =>
            {
                continue;
            }
            Some(gemini::PartData::InlineData { inline_data, .. }) => {
                append(
                    &mut rest,
                    "gemini_inline_data",
                    serde_json::to_value(inline_data)?,
                )?;
            }
            Some(gemini::PartData::FileData { file_data, .. }) => {
                append(
                    &mut rest,
                    "gemini_file_data",
                    serde_json::to_value(file_data)?,
                )?;
            }
            Some(gemini::PartData::Raw(raw)) => {
                append(&mut rest, "gemini_raw_parts", raw)?;
            }
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
            rest,
        },
        has_tool,
    ))
}

fn append(
    rest: &mut openai::Rest,
    key: &'static str,
    value: serde_json::Value,
) -> Result<(), TransformError> {
    match rest.get_mut(key) {
        Some(serde_json::Value::Array(values)) => values.push(value),
        Some(_) => {
            return Err(TransformError::shape(
                "Gemini stream content",
                format!("{key} extension is not an array"),
            ));
        }
        None => {
            rest.insert(key.into(), serde_json::Value::Array(vec![value]));
        }
    }
    Ok(())
}

fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
