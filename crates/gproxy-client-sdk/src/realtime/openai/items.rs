use super::*;
pub(in crate::realtime) fn create(item: o::ConversationItem) -> o::ConversationItemCreateEvent {
    o::ConversationItemCreateEvent::builder(
        item,
        o::ConversationItemCreateEventType::ConversationItemCreate,
    )
    .build()
}
pub(in crate::realtime) fn text(
    id: Option<&str>,
    role: &Role,
    text: &str,
) -> Result<o::ConversationItemCreateEvent> {
    use o::ConversationItem as I;
    let item = match role {
        Role::User => {
            let part = o::RealtimeConversationItemUserMessageContent::builder()
                .type_(Some(
                    o::RealtimeConversationItemUserMessageContentType::InputText,
                ))
                .text(Some(text.to_owned()))
                .build();
            let mut m = o::RealtimeConversationItemUserMessage::builder(
                vec![part],
                o::RealtimeConversationItemUserMessageRole::User,
                o::RealtimeConversationItemUserMessageType::Message,
            )
            .build();
            m.id = id.map(|s| Some(s.to_owned()));
            I::RealtimeConversationItemUserMessage(m)
        }
        Role::Assistant => {
            let part = o::RealtimeConversationItemAssistantMessageContent::builder()
                .type_(Some(
                    o::RealtimeConversationItemAssistantMessageContentType::OutputText,
                ))
                .text(Some(text.to_owned()))
                .build();
            let mut m = o::RealtimeConversationItemAssistantMessage::builder(
                vec![part],
                o::RealtimeConversationItemAssistantMessageRole::Assistant,
                o::RealtimeConversationItemAssistantMessageType::Message,
            )
            .build();
            m.id = id.map(|s| Some(s.to_owned()));
            I::RealtimeConversationItemAssistantMessage(m)
        }
        Role::System => {
            let part = o::RealtimeConversationItemSystemMessageContent::builder()
                .type_(Some(
                    o::RealtimeConversationItemSystemMessageContentType::InputText,
                ))
                .text(Some(text.to_owned()))
                .build();
            let mut m = o::RealtimeConversationItemSystemMessage::builder(
                vec![part],
                o::RealtimeConversationItemSystemMessageRole::System,
                o::RealtimeConversationItemSystemMessageType::Message,
            )
            .build();
            m.id = id.map(|s| Some(s.to_owned()));
            I::RealtimeConversationItemSystemMessage(m)
        }
        _ => {
            return Err(Error::invalid(
                "Realtime text history needs a user, assistant or system role",
            ));
        }
    };
    Ok(create(item))
}
pub(in crate::realtime) fn tool_result(
    result: &ToolResult,
) -> Result<o::ConversationItemCreateEvent> {
    let output = super::super::gemini::tool_output(result)?;
    let output = match output {
        Value::String(text) => text,
        value => value.to_string(),
    };
    Ok(create(
        o::ConversationItem::RealtimeConversationItemFunctionCallOutput(
            o::RealtimeConversationItemFunctionCallOutput::builder(
                result.id.clone(),
                output,
                o::RealtimeConversationItemFunctionCallOutputType::FunctionCallOutput,
            )
            .build(),
        ),
    ))
}
pub(in crate::realtime) fn history(
    config: &RealtimeConfig,
    warnings: &mut Vec<Warning>,
) -> Result<Vec<Value>> {
    let Some(Conversation::V1 { messages }) = &config.history else {
        return Ok(Vec::new());
    };
    let mut frames = Vec::new();
    for message in messages {
        for block in message.model_content() {
            if block.cache.is_some() {
                warn(
                    warnings,
                    "history.cache",
                    "Realtime does not support cache breakpoints",
                );
            }
            if !block.provider_metadata.is_empty() {
                warn(
                    warnings,
                    "history.provider_metadata",
                    "Prior-session metadata is not sent as new-session item metadata",
                );
            }
            let event = match &block.content {
                Content::Text(t) => text(None, &message.role, &t.text)?,
                Content::ToolCall(call) => {
                    let mut native = o::RealtimeConversationItemFunctionCall::builder(
                        call.arguments.clone(),
                        call.name.clone(),
                        o::RealtimeConversationItemFunctionCallType::FunctionCall,
                    )
                    .build();
                    native.call_id = Some(Some(call.id.clone()));
                    create(o::ConversationItem::RealtimeConversationItemFunctionCall(
                        native,
                    ))
                }
                Content::ToolResult(result) => tool_result(result)?,
                _ => {
                    return Err(Error::invalid(
                        "Realtime history supports text and tool blocks; resolve other content before connecting",
                    ));
                }
            };
            frames.push(json(event)?);
        }
    }
    Ok(frames)
}
