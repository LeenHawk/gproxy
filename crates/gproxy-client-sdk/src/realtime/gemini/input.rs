use super::*;
pub(in crate::realtime) fn realtime(
    input: l::BidiGenerateContentRealtimeInput,
) -> l::LiveClientMessage {
    l::LiveClientMessage::builder()
        .realtime_input(input)
        .build()
}
pub(in crate::realtime) fn text(text: &str, turn_complete: bool) -> l::LiveClientMessage {
    l::LiveClientMessage::builder()
        .client_content(
            l::BidiGenerateContentClientContent::builder()
                .turns(vec![
                    l::LiveContent::builder()
                        .role("user")
                        .parts(vec![l::LivePart::builder().text(text).build()])
                        .build(),
                ])
                .turn_complete(turn_complete)
                .build(),
        )
        .build()
}
pub(in crate::realtime) fn tool_output(result: &ToolResult) -> Result<Value> {
    let output = match &result.output {
        ToolOutput::Json(value) => value.clone(),
        ToolOutput::Blocks(blocks) => {
            let mut text = String::new();
            for block in blocks {
                match &block.content {
                    ToolResultContent::Text(value) => text.push_str(&value.text),
                    _ => {
                        return Err(Error::invalid(
                            "Realtime tool results support JSON and text blocks",
                        ));
                    }
                }
            }
            Value::String(text)
        }
        ToolOutput::Unknown(_) => {
            return Err(Error::invalid("unknown Realtime tool result output"));
        }
    };
    Ok(if result.is_error {
        serde_json::json!({"error": output})
    } else {
        output
    })
}
pub(in crate::realtime) fn tool_result(result: &ToolResult) -> Result<l::LiveClientMessage> {
    let output = tool_output(result)?;
    let response = match output {
        Value::Object(map) => map,
        value => [("result".into(), value)].into_iter().collect(),
    };
    let native = g::FunctionResponse::builder(result.name.clone(), response)
        .id(result.id.clone())
        .build();
    Ok(l::LiveClientMessage::builder()
        .tool_response(
            l::BidiGenerateContentToolResponse::builder()
                .function_responses(vec![native])
                .build(),
        )
        .build())
}
pub(in crate::realtime) fn history(
    config: &RealtimeConfig,
    warnings: &mut Vec<Warning>,
) -> Result<Option<Value>> {
    let Some(Conversation::V1 { messages }) = &config.history else {
        return Ok(None);
    };
    let mut turns = Vec::new();
    for message in messages {
        if message.is_record_only() {
            continue;
        }
        let role = match message.role {
            Role::Assistant => "model",
            Role::User | Role::Tool => "user",
            Role::System => continue,
            _ => return Err(Error::invalid("unknown Live history role")),
        };
        let mut parts = Vec::new();
        for block in message.model_content() {
            if block.cache.is_some() {
                warn(
                    warnings,
                    "history.cache",
                    "Live does not support cache breakpoints",
                );
            }
            if !block.provider_metadata.is_empty() {
                warn(
                    warnings,
                    "history.provider_metadata",
                    "Prior-session metadata is not sent to a new Live session",
                );
            }
            parts.push(match &block.content {
                Content::Text(t) => l::LivePart::builder().text(t.text.clone()).build(),
                Content::ToolCall(c) => l::LivePart::builder()
                    .function_call(
                        g::FunctionCall::builder(c.name.clone())
                            .args(
                                serde_json::from_str::<protocol::Rest>(&c.arguments)
                                    .map_err(Error::invalid)?,
                            )
                            .id(c.id.clone())
                            .build(),
                    )
                    .build(),
                Content::ToolResult(r) => {
                    let response = match tool_output(r)? {
                        Value::Object(map) => map,
                        v => [("result".into(), v)].into_iter().collect(),
                    };
                    l::LivePart::builder()
                        .function_response(
                            g::FunctionResponse::builder(r.name.clone(), response)
                                .id(r.id.clone())
                                .build(),
                        )
                        .build()
                }
                _ => return Err(Error::invalid("Live history supports text and tool blocks")),
            });
        }
        if !parts.is_empty() {
            turns.push(l::LiveContent::builder().role(role).parts(parts).build());
        }
    }
    if turns.is_empty() {
        return Ok(None);
    }
    Ok(Some(json(
        l::LiveClientMessage::builder()
            .client_content(
                l::BidiGenerateContentClientContent::builder()
                    .turns(turns)
                    .turn_complete(false)
                    .build(),
            )
            .build(),
    )?))
}
