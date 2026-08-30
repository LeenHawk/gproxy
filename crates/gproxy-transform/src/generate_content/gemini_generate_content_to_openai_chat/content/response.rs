use std::collections::{BTreeMap, VecDeque};

use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::parts::merge;
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
    let mut pending = VecDeque::new();
    let mut by_id = BTreeMap::new();
    for (index, part) in content.parts.into_iter().enumerate() {
        let thought = part.thought == Some(true);
        match part.data {
            Some(gemini::PartData::Text { text: value, .. }) if thought => reasoning.push(value),
            Some(gemini::PartData::Text { text: value, .. }) => text.push(value),
            Some(gemini::PartData::FunctionCall {
                function_call,
                rest,
            }) => {
                let id = function_call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("gemini_call_{candidate}_{index}"));
                calls.push(function_call_to_chat(
                    function_call,
                    id,
                    merge(part.rest, rest),
                )?);
            }
            Some(gemini::PartData::ExecutableCode {
                executable_code,
                rest,
            }) => {
                let id = executable_code
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("gemini_code_{candidate}_{index}"));
                let call_index = calls.len();
                by_id.insert(id.clone(), call_index);
                pending.push_back(call_index);
                calls.push(code_call(executable_code, id, merge(part.rest, rest))?);
            }
            Some(gemini::PartData::CodeExecutionResult {
                code_execution_result,
                ..
            }) => {
                let call_index = match code_execution_result.id.as_ref() {
                    Some(id) => {
                        let call_index = by_id.remove(id).ok_or_else(|| {
                            TransformError::shape(
                                "Gemini code execution result",
                                "id has no preceding executableCode",
                            )
                        })?;
                        if let Some(position) =
                            pending.iter().position(|value| *value == call_index)
                        {
                            pending.remove(position);
                        }
                        call_index
                    }
                    None => {
                        let call_index = pending.pop_front().ok_or_else(|| {
                            TransformError::shape(
                                "Gemini code execution result",
                                "no preceding executableCode",
                            )
                        })?;
                        by_id.retain(|_, value| *value != call_index);
                        call_index
                    }
                };
                attach_result(&mut calls[call_index], code_execution_result)?;
            }
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
        rest: content.rest,
    })
}

fn empty() -> openai::ChatMessage {
    openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: None,
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
    rest: openai::Rest,
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
                rest: call.rest,
            },
            rest,
        },
    ))
}

fn code_call(
    code: gemini::ExecutableCode,
    id: String,
    rest: openai::Rest,
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
            rest,
        },
    ))
}

fn attach_result(
    call: &mut openai::ChatToolCall,
    result: gemini::CodeExecutionResult,
) -> Result<(), TransformError> {
    let openai::ChatToolCall::Function(call) = call else {
        return Err(TransformError::shape(
            "Chat code execution call",
            "expected function tool call",
        ));
    };
    call.rest.insert(
        "gemini_code_execution_result".into(),
        serde_json::to_value(result)?,
    );
    Ok(())
}
