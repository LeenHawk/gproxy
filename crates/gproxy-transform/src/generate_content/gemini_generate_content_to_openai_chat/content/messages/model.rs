use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;

use super::State;

impl State {
    pub(super) fn model(
        &mut self,
        content: gemini::Content,
        turn: usize,
    ) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
        let mut text = Vec::new();
        let mut reasoning = Vec::new();
        let mut calls = Vec::new();
        let mut results = Vec::new();
        for (index, part) in content.parts.into_iter().enumerate() {
            let thought = part.thought == Some(true);
            match part.data {
                Some(gemini::PartData::Text { text: value, .. }) if thought => {
                    reasoning.push(value)
                }
                Some(gemini::PartData::Text { text: value, .. }) => text.push(value),
                Some(gemini::PartData::FunctionCall { function_call, .. }) => {
                    let id = function_call
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("gemini_call_{turn}_{index}"));
                    self.calls
                        .entry(function_call.name.clone())
                        .or_default()
                        .push_back(id.clone());
                    calls.push(function_call_to_chat(function_call, id)?);
                }
                Some(gemini::PartData::ExecutableCode {
                    executable_code, ..
                }) => {
                    let id = executable_code
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("gemini_code_{turn}_{index}"));
                    self.pending_code.push_back(id.clone());
                    calls.push(code_call(executable_code, id)?);
                }
                Some(gemini::PartData::CodeExecutionResult {
                    code_execution_result,
                    ..
                }) => {
                    let id = match code_execution_result.id.clone() {
                        Some(id) => {
                            let position = self
                                .pending_code
                                .iter()
                                .position(|pending| pending == &id)
                                .ok_or_else(|| {
                                    TransformError::shape(
                                        "Gemini code execution result",
                                        "id has no preceding executableCode",
                                    )
                                })?;
                            self.pending_code.remove(position);
                            id
                        }
                        None => self.pending_code.pop_front().ok_or_else(|| {
                            TransformError::shape(
                                "Gemini code execution result",
                                "no preceding executableCode",
                            )
                        })?,
                    };
                    results.push(tool_message(
                        id,
                        serde_json::to_string(&code_execution_result)?,
                    ));
                }
                Some(gemini::PartData::Raw(_)) => {}
                Some(other) => {
                    return Err(TransformError::unsupported(
                        "Gemini model part",
                        serde_json::to_string(&other)?,
                    ));
                }
                None => {}
            }
        }
        let mut output = Vec::new();
        if !text.is_empty() || !reasoning.is_empty() || !calls.is_empty() {
            output.push(assistant(text, reasoning, calls));
        }
        output.extend(results);
        Ok(output)
    }
}

fn assistant(
    text: Vec<String>,
    reasoning: Vec<String>,
    calls: Vec<openai::ChatToolCall>,
) -> openai::ChatCompletionMessageParam {
    openai::ChatCompletionMessageParam::Assistant(openai::ChatAssistantMessageParam {
        role: openai::ChatAssistantRole::Assistant,
        content: (!text.is_empty()).then(|| openai::ChatAssistantContent::Text(text.join(""))),
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: (!reasoning.is_empty()).then(|| reasoning.join("")),
        refusal: None,
        tool_calls: (!calls.is_empty()).then_some(calls),
        rest: Default::default(),
    })
}

fn function_call_to_chat(
    call: gemini::FunctionCall,
    id: String,
) -> Result<openai::ChatToolCall, TransformError> {
    let arguments = call
        .args
        .ok_or_else(|| TransformError::shape("Gemini function call", "args is missing"))?;
    Ok(openai::ChatToolCall::Function(
        openai::ChatFunctionToolCall {
            id,
            type_: openai::FunctionToolChoiceType::Function,
            function: openai::FunctionCall {
                arguments: serde_json::to_string(&arguments)?,
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

pub(super) fn tool_message(id: String, content: String) -> openai::ChatCompletionMessageParam {
    openai::ChatCompletionMessageParam::Tool(openai::ChatToolMessageParam {
        role: openai::ChatToolRole::Tool,
        content: openai::ChatTextContent::Text(content),
        tool_call_id: id,
        rest: Default::default(),
    })
}
