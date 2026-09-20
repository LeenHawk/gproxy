use crate::{
    transform::{Report, TransformError},
    wire::openai::{chat as c, responses::input as r},
};

fn text(value: c::TextContent) -> String {
    match value {
        c::TextContent::Text(value) => value,
        c::TextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| part.text)
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub(super) fn id(value: String) -> Result<String, TransformError> {
    if value.is_empty() {
        Err(TransformError::shape("call_id", "empty tool identity"))
    } else {
        Ok(value)
    }
}

pub(super) fn easy(role: r::MessageRole, text: String) -> r::InputItem {
    r::InputItem::Easy(r::EasyInputMessage::builder(r::MessageContent::Text(text), role).build())
}

pub(super) fn to_responses(
    messages: Vec<c::ChatMessage>,
    prior_calls: &std::collections::BTreeMap<String, super::ToolCallKind>,
    _report: &mut Report,
    flow: &mut crate::transform::identity::IdentityFlow,
    policy: &crate::transform::identity::TargetIdPolicy,
) -> Result<Vec<r::InputItem>, TransformError> {
    use crate::transform::identity::{IdentityRole, SourceIdentity};
    let mut calls = std::collections::BTreeMap::new();
    for message in &messages {
        if let c::ChatMessage::Assistant(message) = message {
            for call in message.tool_calls.iter().flatten() {
                let (source, is_custom) = match call {
                    c::MessageToolCall::Function(call) => (&call.id, false),
                    c::MessageToolCall::Custom(call) => (&call.id, true),
                };
                if source.is_empty() || calls.contains_key(source) {
                    return Err(TransformError::shape(
                        "call_id",
                        "empty or duplicate tool call identity",
                    ));
                }
                let index = calls.len() as u64;
                let handle = flow
                    .resolve_as(
                        IdentityRole::ToolCall,
                        IdentityRole::ToolCall,
                        SourceIdentity::new(
                            crate::Dialect::OpenAiChat,
                            Some(source.clone()),
                            index,
                        ),
                        policy,
                    )
                    .map_err(|error| {
                        TransformError::shape("request.identity", error.to_string())
                    })?;
                calls.insert(source.clone(), (handle.emitted_id, is_custom));
            }
        }
    }
    for message in &messages {
        let c::ChatMessage::Tool(message) = message else {
            continue;
        };
        let Some(kind) = prior_calls.get(&message.tool_call_id) else {
            continue;
        };
        if message.tool_call_id.is_empty() {
            return Err(TransformError::shape(
                "tool_call_id",
                "empty prior call identity",
            ));
        }
        let is_custom = *kind == super::ToolCallKind::Custom;
        if let Some((_, existing)) = calls.get(&message.tool_call_id) {
            if *existing != is_custom {
                return Err(TransformError::shape(
                    "call.kind",
                    "saved kind conflicts with declared history",
                ));
            }
            continue;
        }
        let handle = flow
            .resolve_or_allocate(
                IdentityRole::ToolCall,
                SourceIdentity::new(
                    crate::Dialect::OpenAiChat,
                    Some(message.tool_call_id.clone()),
                    calls.len() as u64,
                ),
                policy,
            )
            .map_err(|error| TransformError::shape("request.identity", error.to_string()))?;
        calls.insert(message.tool_call_id.clone(), (handle.emitted_id, is_custom));
    }
    let mut legacy_pending = std::collections::BTreeMap::new();
    let mut legacy_index = calls.len() as u64;
    let mut items = Vec::new();
    let mut logical_index = 0u64;
    for message in messages {
        match message {
            c::ChatMessage::System(message) => {
                items.push(easy(r::MessageRole::System, text(message.content)))
            }
            c::ChatMessage::Developer(message) => {
                items.push(easy(r::MessageRole::Developer, text(message.content)))
            }
            c::ChatMessage::User(message) => {
                items.push(super::media::user_message(message.content)?)
            }
            c::ChatMessage::Assistant(message) => {
                if let Some(text) = c::visible_reasoning(
                    &message.reasoning_content,
                    &message.reasoning,
                    &message.reasoning_details,
                ) {
                    use crate::transform::identity::OutputItemKind;
                    let id = flow
                        .resolve_or_allocate(
                            IdentityRole::OutputItem(OutputItemKind::Reasoning),
                            SourceIdentity::new(
                                crate::Dialect::OpenAiChat,
                                None,
                                items.len() as u64,
                            ),
                            policy,
                        )
                        .map_err(|e| TransformError::shape("reasoning.id", e.to_string()))?
                        .emitted_id;
                    let mut item = r::ReasoningItem::builder(
                        r::ReasoningItemType::ReasoningItem,
                        id,
                        Vec::new(),
                    )
                    .build();
                    item.content = Some(vec![
                        r::ReasoningContent::builder(r::ReasoningTextType::ReasoningText, text)
                            .build(),
                    ]);
                    items.push(r::InputItem::Reasoning(item));
                }
                if message.audio.flatten().is_some() {
                    continue;
                }
                let legacy = message.function_call.flatten();
                if legacy.is_some()
                    && message
                        .tool_calls
                        .as_ref()
                        .is_some_and(|calls| !calls.is_empty())
                {
                    return Err(TransformError::shape(
                        "messages.function_call",
                        "legacy/current calls coexist",
                    ));
                }
                let mut parts = Vec::new();
                if let Some(Some(content)) = message.content {
                    match content {
                        c::AssistantContent::Text(text) => parts.push(output_text(text)),
                        c::AssistantContent::Parts(content) => {
                            for part in content {
                                parts.push(match part {
                                    c::AssistantContentPart::Text(part) => output_text(part.text),
                                    c::AssistantContentPart::Refusal(part) => {
                                        output_refusal(part.refusal)
                                    }
                                });
                            }
                        }
                    }
                }
                if let Some(Some(refusal)) = message.refusal {
                    parts.push(output_refusal(refusal));
                }
                if !parts.is_empty() {
                    use crate::transform::identity::{
                        IdentityRole, OutputItemKind, SourceIdentity,
                    };
                    let handle = flow
                        .resolve_as(
                            IdentityRole::Message,
                            IdentityRole::OutputItem(OutputItemKind::Message),
                            SourceIdentity::new(crate::Dialect::OpenAiChat, None, logical_index),
                            policy,
                        )
                        .map_err(|error| {
                            TransformError::shape("request.identity", error.to_string())
                        })?;
                    logical_index += 1;
                    items.push(r::InputItem::OutputMessage(
                        r::ResponseOutputMessage::builder(
                            handle.emitted_id,
                            parts,
                            r::OutputMessageRole::Assistant,
                            r::OutputMessageStatus::Completed,
                            r::MessageType::Message,
                        )
                        .build(),
                    ));
                }
                if let Some(call) = legacy {
                    if legacy_pending.contains_key(&call.name) {
                        return Err(TransformError::missing_metadata(
                            "ambiguous legacy function result correlation",
                        ));
                    }
                    let handle = flow
                        .resolve_as(
                            IdentityRole::ToolCall,
                            IdentityRole::ToolCall,
                            SourceIdentity::new(crate::Dialect::OpenAiChat, None, legacy_index),
                            policy,
                        )
                        .map_err(|error| {
                            TransformError::shape("request.identity", error.to_string())
                        })?;
                    legacy_index += 1;
                    legacy_pending.insert(call.name.clone(), handle.emitted_id.clone());
                    items.push(r::InputItem::FunctionCall(
                        r::FunctionCall::builder(
                            r::FunctionCallType::FunctionCall,
                            call.arguments,
                            handle.emitted_id,
                            call.name,
                        )
                        .build(),
                    ));
                }
                for call in message.tool_calls.unwrap_or_default() {
                    items.push(match call {
                        c::MessageToolCall::Function(call) => r::InputItem::FunctionCall(
                            r::FunctionCall::builder(
                                r::FunctionCallType::FunctionCall,
                                call.function.arguments,
                                calls.get(&call.id).expect("prebound").0.clone(),
                                call.function.name,
                            )
                            .build(),
                        ),
                        c::MessageToolCall::Custom(call) => r::InputItem::CustomToolCall(
                            r::CustomToolCall::builder(
                                r::CustomToolCallType::CustomToolCall,
                                calls.get(&call.id).expect("prebound").0.clone(),
                                call.custom.input,
                                call.custom.name,
                            )
                            .build(),
                        ),
                    });
                }
            }
            c::ChatMessage::Tool(message) => {
                let (call_id, is_custom) =
                    calls.get(&message.tool_call_id).cloned().ok_or_else(|| {
                        TransformError::missing_metadata(
                            "tool result requires prior call identity/type",
                        )
                    })?;
                let output = text(message.content);
                items.push(if is_custom {
                    r::InputItem::CustomToolCallOutput(
                        r::CustomToolCallOutput::builder(
                            r::CustomToolCallOutputType::CustomToolCallOutput,
                            call_id,
                            r::CustomOutput::Text(output),
                        )
                        .build(),
                    )
                } else {
                    r::InputItem::FunctionCallOutput(
                        r::FunctionCallOutput::builder(
                            r::FunctionCallOutputType::FunctionCallOutput,
                            call_id,
                            r::FunctionOutput::Text(output),
                        )
                        .build(),
                    )
                });
            }
            c::ChatMessage::Function(message) => {
                let call_id = legacy_pending.remove(&message.name).ok_or_else(|| {
                    TransformError::missing_metadata("legacy function result correlation")
                })?;
                let output = message.content.ok_or_else(|| {
                    TransformError::unsupported(
                        "messages.function.content",
                        "Responses function output cannot represent null text",
                    )
                })?;
                items.push(r::InputItem::FunctionCallOutput(
                    r::FunctionCallOutput::builder(
                        r::FunctionCallOutputType::FunctionCallOutput,
                        call_id,
                        r::FunctionOutput::Text(output),
                    )
                    .build(),
                ));
            }
        }
    }
    Ok(items)
}

fn output_text(text: String) -> r::OutputContent {
    r::OutputContent::Text(
        r::ResponseOutputText::builder(
            r::ResponseOutputTextType::ResponseOutputText,
            text,
            Vec::new(),
            Vec::new(),
        )
        .build(),
    )
}

fn output_refusal(text: String) -> r::OutputContent {
    r::OutputContent::Refusal(
        r::ResponseOutputRefusal::builder(
            r::ResponseOutputRefusalType::ResponseOutputRefusal,
            text,
        )
        .build(),
    )
}
