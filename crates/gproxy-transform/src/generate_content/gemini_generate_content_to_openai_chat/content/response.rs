use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;

pub(crate) fn message(
    content: Option<gemini::Content>,
    candidate: usize,
) -> Result<openai::ChatMessage, TransformError> {
    let Some(content) = content else {
        return Ok(empty());
    };
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut calls = Vec::new();
    for (index, part) in content.parts.into_iter().enumerate() {
        let thought = part.thought == Some(true);
        match part.data {
            Some(gemini::PartData::Text { text: value, .. }) if thought => reasoning.push(value),
            Some(gemini::PartData::Text { text: value, .. }) => text.push(value),
            Some(gemini::PartData::FunctionCall { function_call, .. }) => {
                let id = function_call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("gemini_call_{candidate}_{index}"));
                calls.push(function_call_to_chat(function_call, id)?);
            }
            Some(gemini::PartData::ExecutableCode {
                executable_code, ..
            }) => {
                let id = executable_code
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("gemini_code_{candidate}_{index}"));
                calls.push(code_call(executable_code, id)?);
            }
            Some(gemini::PartData::CodeExecutionResult { .. }) => {}
            Some(gemini::PartData::InlineData { .. })
            | Some(gemini::PartData::FileData { .. })
            | Some(gemini::PartData::FunctionResponse { .. })
            | Some(gemini::PartData::ToolCall { .. })
            | Some(gemini::PartData::ToolResponse { .. })
            | Some(gemini::PartData::Raw(_)) => {}
            Some(other) => {
                return Err(TransformError::unsupported(
                    "Gemini response part",
                    serde_json::to_string(&other)?,
                ));
            }
            None => {}
        }
    }
    Ok(openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: (!text.is_empty()).then(|| text.join("")),
        refusal: None,
        annotations: None,
        audio: None,
        function_call: None,
        reasoning_content: (!reasoning.is_empty()).then(|| reasoning.join("")),
        tool_calls: (!calls.is_empty()).then_some(calls),
        rest: Default::default(),
    })
}
fn empty() -> openai::ChatMessage {
    openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: Some(String::new()),
        refusal: None,
        annotations: None,
        audio: None,
        function_call: None,
        reasoning_content: None,
        tool_calls: None,
        rest: Default::default(),
    }
}

fn function_call_to_chat(
    call: gemini::FunctionCall,
    id: String,
) -> Result<openai::ChatToolCall, TransformError> {
    let args = call
        .args
        .ok_or_else(|| TransformError::shape("Gemini function call", "args is missing"))?;
    Ok(openai::ChatToolCall::Function(
        openai::ChatFunctionToolCall {
            id,
            type_: openai::FunctionToolChoiceType::Function,
            function: openai::FunctionCall {
                arguments: serde_json::to_string(&args)?,
                name: call.name,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
    ))
}
fn code_call(
    code: gemini::ExecutableCode,
    id: String,
) -> Result<openai::ChatToolCall, TransformError> {
    Ok(openai::ChatToolCall::Function(
        openai::ChatFunctionToolCall {
            id,
            type_: openai::FunctionToolChoiceType::Function,
            function: openai::FunctionCall {
                arguments: serde_json::to_string(&code)?,
                name: CODE_EXECUTION_NAME.into(),
                rest: Default::default(),
            },
            rest: Default::default(),
        },
    ))
}
