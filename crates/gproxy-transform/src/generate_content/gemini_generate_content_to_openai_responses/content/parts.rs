use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::media::{file_message, media_message};
use super::messages::MessagePart;
use super::{ContentConverter, wire};

impl ContentConverter {
    pub(super) fn part(
        &mut self,
        mut part: gemini::Part,
        response: bool,
        message: &mut Vec<MessagePart>,
    ) -> Result<Option<openai::ResponseItem>, TransformError> {
        let rest = wire::part_rest(&mut part)?;
        let signature = part.thought_signature.take();
        let Some(data) = part.data.take() else {
            return Ok(signature.map(|value| self.reasoning(None, Some(value), rest)));
        };
        Ok(match data {
            gemini::PartData::Text {
                text,
                rest: data_rest,
            } if part.thought == Some(true) => {
                Some(self.reasoning(Some(text), signature, merge(rest, data_rest)))
            }
            gemini::PartData::Text {
                text,
                rest: data_rest,
            } => {
                if let Some(signature) = signature {
                    message.push(text_message(text, response, merge(rest.clone(), data_rest)));
                    Some(self.reasoning(None, Some(signature), rest))
                } else {
                    message.push(text_message(text, response, merge(rest, data_rest)));
                    None
                }
            }
            gemini::PartData::InlineData {
                inline_data,
                rest: data_rest,
            } => {
                message.push(media_message(
                    inline_data,
                    response,
                    merge(rest, data_rest),
                )?);
                None
            }
            gemini::PartData::FileData {
                file_data,
                rest: data_rest,
            } => {
                message.push(file_message(file_data, response, merge(rest, data_rest))?);
                None
            }
            gemini::PartData::FunctionCall {
                function_call,
                rest: data_rest,
            } => Some(self.function_call(function_call, signature, merge(rest, data_rest))?),
            gemini::PartData::FunctionResponse {
                function_response,
                rest: data_rest,
            } => Some(self.function_response(function_response, merge(rest, data_rest))?),
            gemini::PartData::ExecutableCode {
                executable_code,
                rest: data_rest,
            } => Some(self.executable_code(executable_code, merge(rest, data_rest))),
            gemini::PartData::CodeExecutionResult {
                code_execution_result,
                rest: data_rest,
            } => Some(self.code_result(code_execution_result, merge(rest, data_rest))?),
            gemini::PartData::ToolCall {
                tool_call,
                rest: data_rest,
            } => Some(self.tool_call(tool_call, merge(rest, data_rest))?),
            gemini::PartData::ToolResponse {
                tool_response,
                rest: data_rest,
            } => Some(self.tool_response(tool_response, merge(rest, data_rest))?),
            gemini::PartData::Raw(raw) => Some(openai::ResponseItem::Unknown(raw)),
            other => Some(openai::ResponseItem::Unknown(serde_json::to_value(other)?)),
        })
    }

    fn reasoning(
        &mut self,
        text: Option<String>,
        signature: Option<String>,
        rest: openai::Rest,
    ) -> openai::ResponseItem {
        let id = super::super::ids::reasoning_id(signature.as_deref(), self.next_reasoning);
        self.next_reasoning = self.next_reasoning.saturating_add(1);
        openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
            id: Some(id),
            summary: Vec::new(),
            content: text.map(|text| {
                vec![openai::ResponseReasoningTextPart {
                    text,
                    type_: "reasoning_text".into(),
                    rest: Default::default(),
                }]
            }),
            encrypted_content: signature,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            rest,
        }))
    }
}

fn text_message(text: String, response: bool, rest: openai::Rest) -> MessagePart {
    if response {
        MessagePart::Output(openai::ResponseMessageOutputContentPart::OutputText(
            openai::ResponseOutputText {
                type_: openai::ResponseOutputTextType::OutputText,
                annotations: Vec::new(),
                logprobs: None,
                text,
                rest,
            },
        ))
    } else {
        MessagePart::Input(openai::ResponseInputContentPart::InputText(
            openai::ResponseInputText {
                type_: openai::ResponseInputTextType::InputText,
                text,
                prompt_cache_breakpoint: None,
                rest,
            },
        ))
    }
}

fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
