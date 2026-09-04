use std::collections::BTreeMap;

use gproxy_protocol::openai;

use crate::TransformError;

use super::content::{assistant_content, text_content, text_output, user_content};

pub(super) fn message_items(
    messages: Vec<openai::ChatCompletionMessageParam>,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    let mut correlations = BTreeMap::new();
    let mut output = Vec::new();
    for (index, message) in messages.into_iter().enumerate() {
        output.extend(message_to_items(index, message, &mut correlations)?);
    }
    Ok(output)
}

fn message_to_items(
    index: usize,
    message: openai::ChatCompletionMessageParam,
    correlations: &mut BTreeMap<String, (String, ToolKind)>,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    match message {
        openai::ChatCompletionMessageParam::Developer(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::Developer,
            text_content(message.content)?,
        )]),
        openai::ChatCompletionMessageParam::System(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::System,
            text_content(message.content)?,
        )]),
        openai::ChatCompletionMessageParam::User(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::User,
            user_content(message.content)?,
        )]),
        openai::ChatCompletionMessageParam::Assistant(message) => {
            assistant_items(index, message, correlations)
        }
        openai::ChatCompletionMessageParam::Tool(message) => {
            let (call_id, kind) = correlations
                .get(&message.tool_call_id)
                .cloned()
                .unwrap_or_else(|| (response_call_id(&message.tool_call_id), ToolKind::Function));
            Ok(vec![tool_output(
                kind,
                call_id,
                text_output(message.content)?,
            )])
        }
        openai::ChatCompletionMessageParam::Function(message) => {
            let content = message.content.ok_or_else(|| {
                TransformError::unsupported("OpenAI Chat function message", "null content")
            })?;
            Ok(vec![tool_output(
                ToolKind::Function,
                legacy_call_id(&message.name),
                openai::ResponseOutput::Text(content),
            )])
        }
        openai::ChatCompletionMessageParam::Unknown(_) => Ok(Vec::new()),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

fn assistant_items(
    index: usize,
    message: openai::ChatAssistantMessageParam,
    correlations: &mut BTreeMap<String, (String, ToolKind)>,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    let mut output = Vec::new();
    if let Some(content) = message.content {
        let breakpoint = assistant_has_breakpoint(&content);
        let content = assistant_content(content)?;
        if breakpoint {
            output.push(easy_message(
                openai::ResponseEasyInputMessageRole::Assistant,
                content,
            ));
        } else {
            output.push(output_message(index, content, message.refusal)?);
        }
    } else if let Some(refusal) = message.refusal.filter(|value| !value.is_empty()) {
        output.push(output_message(
            index,
            openai::ResponseEasyInputContent::OutputParts(Vec::new()),
            Some(refusal),
        )?);
    }
    if let Some(reasoning) = message.reasoning_content {
        output.push(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::Reasoning {
                id: None,
                summary: Vec::new(),
                content: Some(vec![crate::wire!(openai::ResponseReasoningTextPart {
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    text: reasoning,
                    rest: Default::default(),
                })]),
                encrypted_content: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: Default::default(),
            },
        )));
    }
    if let Some(call) = message.function_call {
        let call_id = legacy_call_id(&call.name);
        correlations.insert(call_id.clone(), (call_id.clone(), ToolKind::Function));
        output.push(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::FunctionCall {
                arguments: call.arguments,
                call_id: call_id.clone(),
                name: call.name,
                id: Some(response_item_id(&call_id)),
                caller: None,
                namespace: None,
                async_: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: Default::default(),
            },
        )));
    }
    for call in message.tool_calls.into_iter().flatten() {
        output.push(match call {
            openai::ChatToolCall::Function(call) => {
                let call_id = response_call_id(&call.id);
                correlations.insert(call.id.clone(), (call_id.clone(), ToolKind::Function));
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCall {
                    arguments: call.function.arguments,
                    call_id,
                    name: call.function.name,
                    id: Some(response_item_id(&call.id)),
                    caller: None,
                    namespace: None,
                    async_: None,
                    status: Some(openai::ResponseItemLifecycleStatus::Completed),
                    rest: Default::default(),
                }))
            }
            openai::ChatToolCall::Custom(call) => {
                let call_id = response_call_id(&call.id);
                correlations.insert(call.id.clone(), (call_id.clone(), ToolKind::Custom));
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    input: call.custom.input,
                    name: call.custom.name,
                    id: None,
                    caller: None,
                    namespace: None,
                    async_: None,
                    rest: Default::default(),
                }))
            }
            openai::ChatToolCall::Unknown(_) => continue,
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        });
    }
    if output.is_empty() {
        output.push(easy_message(
            openai::ResponseEasyInputMessageRole::Assistant,
            openai::ResponseEasyInputContent::Text(String::new()),
        ));
    }
    Ok(output)
}

fn assistant_has_breakpoint(content: &openai::ChatAssistantContent) -> bool {
    match content {
        openai::ChatAssistantContent::Text(_) | openai::ChatAssistantContent::Unknown(_) => false,
        openai::ChatAssistantContent::Parts(parts) => parts.iter().any(|part| match part {
            openai::ChatAssistantContentPart::Text(part) => part.prompt_cache_breakpoint.is_some(),
            openai::ChatAssistantContentPart::Refusal(part) => {
                part.prompt_cache_breakpoint.is_some()
            }
            openai::ChatAssistantContentPart::Unknown(_) => false,
            #[cfg(not(feature = "exhaustive"))]
            _ => false,
        }),
        #[cfg(not(feature = "exhaustive"))]
        _ => false,
    }
}

fn output_message(
    index: usize,
    content: openai::ResponseEasyInputContent,
    refusal: Option<String>,
) -> Result<openai::ResponseItem, TransformError> {
    let openai::ResponseEasyInputContent::OutputParts(mut content) = content else {
        return Err(TransformError::shape(
            "Chat assistant history",
            "assistant content did not produce output parts",
        ));
    };
    if let Some(refusal) = refusal.filter(|value| !value.is_empty()) {
        content.push(openai::ResponseMessageOutputContentPart::Refusal(
            crate::wire!(openai::ResponseRefusal {
                type_: openai::ResponseRefusalType::Refusal,
                refusal,
                rest: Default::default(),
            }),
        ));
    }
    Ok(openai::ResponseItem::Message(
        openai::ResponseMessageItem::Output(crate::wire!(openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id: format!("msg_{index}"),
            role: openai::ResponseOutputMessageRole::Assistant,
            content,
            status: openai::ResponseItemLifecycleStatus::Completed,
            phase: None,
            rest: Default::default(),
        })),
    ))
}

fn easy_message(
    role: openai::ResponseEasyInputMessageRole,
    content: openai::ResponseEasyInputContent,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(crate::wire!(
        openai::ResponseEasyInputMessageItem {
            type_: Some(openai::ResponseMessageItemType::Message),
            role,
            content,
            phase: None,
            rest: Default::default(),
        }
    )))
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
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}

#[derive(Clone, Copy)]
enum ToolKind {
    Function,
    Custom,
}

fn tool_output(
    kind: ToolKind,
    call_id: String,
    output: openai::ResponseOutput,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(match kind {
        ToolKind::Function => openai::TypedResponseItem::FunctionCallOutput {
            call_id,
            output,
            id: None,
            caller: None,
            name: None,
            namespace: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: Default::default(),
        },
        ToolKind::Custom => openai::TypedResponseItem::CustomToolCallOutput {
            call_id,
            output,
            id: None,
            caller: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: Default::default(),
        },
    }))
}

fn legacy_call_id(name: &str) -> String {
    format!("call_{name}")
}

fn response_call_id(original: &str) -> String {
    prefixed_id(original, "call_")
}

fn response_item_id(original: &str) -> String {
    prefixed_id(original, "fc_")
}

fn prefixed_id(original: &str, prefix: &str) -> String {
    if original.starts_with(prefix.trim_end_matches('_')) {
        return original.to_owned();
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in original.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}{hash:016x}")
}
