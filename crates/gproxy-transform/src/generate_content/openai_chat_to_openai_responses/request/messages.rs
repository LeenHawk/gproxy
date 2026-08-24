use gproxy_protocol::openai;

use crate::TransformError;

use super::content::{assistant_content, text_content, text_output, user_content};

pub(super) fn message_items(
    message: openai::ChatCompletionMessageParam,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    match message {
        openai::ChatCompletionMessageParam::Developer(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::Developer,
            text_content(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::System(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::System,
            text_content(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::User(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::User,
            user_content(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::Assistant(message) => assistant_items(message),
        openai::ChatCompletionMessageParam::Tool(message) => Ok(vec![function_output(
            message.tool_call_id,
            text_output(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::Function(message) => Ok(vec![function_output(
            message.name,
            openai::ResponseOutput::Text(message.content),
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::Unknown(raw) => {
            Ok(vec![openai::ResponseItem::Unknown(raw)])
        }
    }
}

fn assistant_items(
    message: openai::ChatAssistantMessageParam,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    let mut output = Vec::new();
    if let Some(content) = message.content {
        output.push(easy_message(
            openai::ResponseEasyInputMessageRole::Assistant,
            assistant_content(content)?,
            message.rest,
        ));
    }
    if let Some(reasoning) = message.reasoning_content {
        output.push(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::Reasoning {
                id: None,
                summary: Vec::new(),
                content: Some(vec![openai::ResponseReasoningTextPart {
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    text: reasoning,
                    rest: Default::default(),
                }]),
                encrypted_content: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: Default::default(),
            },
        )));
    }
    for call in message.tool_calls.into_iter().flatten() {
        output.push(match call {
            openai::ChatToolCall::Function(call) => {
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCall {
                    arguments: call.function.arguments,
                    call_id: call.id.clone(),
                    name: call.function.name,
                    id: Some(call.id),
                    caller: None,
                    namespace: None,
                    status: Some(openai::ResponseItemLifecycleStatus::Completed),
                    rest: merge(call.rest, call.function.rest),
                }))
            }
            openai::ChatToolCall::Custom(call) => {
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::CustomToolCall {
                    call_id: call.id.clone(),
                    input: call.custom.input,
                    name: call.custom.name,
                    id: Some(call.id),
                    caller: None,
                    namespace: None,
                    rest: merge(call.rest, call.custom.rest),
                }))
            }
            openai::ChatToolCall::Unknown(raw) => openai::ResponseItem::Unknown(raw),
        });
    }
    Ok(output)
}

fn easy_message(
    role: openai::ResponseEasyInputMessageRole,
    content: openai::ResponseEasyInputContent,
    rest: openai::Rest,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
        openai::ResponseEasyInputMessageItem {
            type_: Some(openai::ResponseMessageItemType::Message),
            role,
            content,
            phase: None,
            rest,
        },
    ))
}

pub(super) fn tool_output_part(
    part: openai::ResponseInputContentPart,
) -> Result<openai::ResponseToolOutputContentPart, TransformError> {
    Ok(match part {
        openai::ResponseInputContentPart::InputText(part) => {
            openai::ResponseToolOutputContentPart::InputText(part)
        }
        openai::ResponseInputContentPart::InputImage(part) => {
            openai::ResponseToolOutputContentPart::InputImage(part)
        }
        openai::ResponseInputContentPart::InputFile(part) => {
            openai::ResponseToolOutputContentPart::InputFile(part)
        }
        openai::ResponseInputContentPart::InputAudio(_) => {
            return Err(TransformError::unsupported(
                "Chat tool output",
                "input_audio",
            ));
        }
    })
}

fn function_output(
    call_id: String,
    output: openai::ResponseOutput,
    rest: openai::Rest,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCallOutput {
        call_id,
        output,
        id: None,
        caller: None,
        name: None,
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        created_by: None,
        rest,
    }))
}

fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
