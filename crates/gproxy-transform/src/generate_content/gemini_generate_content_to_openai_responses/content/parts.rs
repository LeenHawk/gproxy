use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::ContentConverter;
use super::media::{file_message, media_message};
use super::messages::MessagePart;

impl ContentConverter {
    pub(super) fn part(
        &mut self,
        mut part: gemini::Part,
        response: bool,
        message: &mut Vec<MessagePart>,
    ) -> Result<Option<openai::ResponseItem>, TransformError> {
        let signature = part.thought_signature.take();
        let Some(data) = part.data.take() else {
            return Ok(signature.map(|value| self.reasoning(None, Some(value))));
        };
        Ok(match data {
            gemini::PartData::Text { text, .. } if part.thought == Some(true) => {
                Some(self.reasoning(Some(text), signature))
            }
            gemini::PartData::Text { text, .. } => {
                if let Some(signature) = signature {
                    message.push(text_message(text, response));
                    Some(self.reasoning(None, Some(signature)))
                } else {
                    message.push(text_message(text, response));
                    None
                }
            }
            gemini::PartData::InlineData { inline_data, .. } => {
                if let Some(part) = media_message(inline_data, response)? {
                    message.push(part);
                }
                None
            }
            gemini::PartData::FileData { file_data, .. } => {
                if let Some(part) = file_message(file_data, response)? {
                    message.push(part);
                }
                None
            }
            gemini::PartData::FunctionCall { function_call, .. } => {
                Some(self.function_call(function_call, signature)?)
            }
            gemini::PartData::FunctionResponse {
                function_response, ..
            } => Some(self.function_response(function_response)?),
            gemini::PartData::ExecutableCode {
                executable_code, ..
            } => Some(self.executable_code(executable_code)?),
            gemini::PartData::CodeExecutionResult {
                code_execution_result,
                ..
            } => Some(self.code_result(code_execution_result)?),
            gemini::PartData::ToolCall { tool_call, .. } => Some(self.tool_call(tool_call)?),
            gemini::PartData::ToolResponse { tool_response, .. } => {
                Some(self.tool_response(tool_response)?)
            }
            gemini::PartData::Raw(_) => None,
            _future => None,
        })
    }

    pub(super) fn reasoning(
        &mut self,
        text: Option<String>,
        signature: Option<String>,
    ) -> openai::ResponseItem {
        let id = super::super::ids::reasoning_id(
            signature.as_deref().or(text.as_deref()),
            self.next_reasoning,
        );
        self.next_reasoning = self.next_reasoning.saturating_add(1);
        openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
            id: Some(id),
            summary: Vec::new(),
            content: text.map(|text| {
                vec![openai::ResponseReasoningTextPart {
                    text,
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    rest: Default::default(),
                }]
            }),
            encrypted_content: signature,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            rest: Default::default(),
        }))
    }
}

fn text_message(text: String, response: bool) -> MessagePart {
    if response {
        MessagePart::Output(openai::ResponseMessageOutputContentPart::OutputText(
            openai::ResponseOutputText {
                type_: openai::ResponseOutputTextType::OutputText,
                annotations: Vec::new(),
                logprobs: None,
                text,
                rest: Default::default(),
            },
        ))
    } else {
        MessagePart::Input(openai::ResponseInputContentPart::InputText(
            openai::ResponseInputText {
                text,
                prompt_cache_breakpoint: None,
                rest: Default::default(),
            },
        ))
    }
}
