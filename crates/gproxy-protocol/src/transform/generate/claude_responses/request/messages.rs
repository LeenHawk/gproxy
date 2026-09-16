use crate::{
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole, OutputItemKind, SourceIdentity, TargetIdPolicy},
    },
    wire::{claude::content as c, openai::responses::input as r},
};

fn easy(role: r::MessageRole, content: r::MessageContent) -> r::InputItem {
    r::InputItem::Easy(r::EasyInputMessage::builder(content, role).build())
}

pub(crate) fn to_responses(
    messages: Vec<c::Message>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
) -> Result<Vec<r::InputItem>, TransformError> {
    let mut out = Vec::new();
    let mut calls = std::collections::BTreeMap::new();
    let mut mcp = std::collections::BTreeMap::new();
    let mut ordinal = 0;
    let mut used = std::collections::BTreeSet::new();
    for message in messages {
        let role = match message.role {
            c::Role::User => r::MessageRole::User,
            c::Role::Assistant => r::MessageRole::Assistant,
            c::Role::System => r::MessageRole::System,
        };
        let blocks = match message.content {
            c::MessageContent::Text(text) => {
                out.push(easy(role, r::MessageContent::Text(text)));
                continue;
            }
            c::MessageContent::Blocks(blocks) => blocks,
        };
        for block in blocks {
            match block {
                c::ContentBlock::Text(text) => {
                    out.push(easy(role, r::MessageContent::Text(text.text)))
                }
                c::ContentBlock::Image(image) => {
                    if role != r::MessageRole::User {
                        continue;
                    }
                    out.push(easy(
                        role,
                        r::MessageContent::Parts(vec![super::media::image(image.source)?]),
                    ));
                }
                c::ContentBlock::Document(doc) => {
                    if role != r::MessageRole::User {
                        continue;
                    }
                    out.push(easy(
                        role,
                        r::MessageContent::Parts(vec![super::media::document(doc)?]),
                    ));
                }
                c::ContentBlock::ToolUse(call) => {
                    if call
                        .caller
                        .as_ref()
                        .is_some_and(|caller| !matches!(caller, c::Caller::Direct(_)))
                    {
                        continue;
                    }
                    if role != r::MessageRole::Assistant
                        || call.id.is_empty()
                        || !used.insert(call.id.clone())
                        || !call.input.is_object()
                    {
                        return Err(TransformError::shape(
                            "tool_use",
                            "assistant object tool call requires unique nonempty id",
                        ));
                    }
                    let source =
                        SourceIdentity::new(crate::Dialect::Claude, Some(call.id.clone()), ordinal);
                    let call_id = flow
                        .resolve_as(
                            IdentityRole::ToolCall,
                            IdentityRole::ToolCall,
                            source.clone(),
                            policy,
                        )
                        .map_err(|e| TransformError::shape("identity", e.to_string()))?
                        .emitted_id;
                    let item_id = flow
                        .resolve_as(
                            IdentityRole::ToolCall,
                            IdentityRole::OutputItem(OutputItemKind::FunctionCall),
                            source,
                            policy,
                        )
                        .map_err(|e| TransformError::shape("identity", e.to_string()))?
                        .emitted_id;
                    ordinal += 1;
                    calls.insert(call.id, call_id.clone());
                    out.push(r::InputItem::FunctionCall(
                        r::FunctionCall::builder(
                            r::FunctionCallType::FunctionCall,
                            serde_json::to_string(&call.input)?,
                            call_id,
                            call.name,
                        )
                        .id(item_id)
                        .build(),
                    ));
                }
                c::ContentBlock::ToolResult(result) => {
                    if role != r::MessageRole::User {
                        return Err(TransformError::shape(
                            "tool_result.role",
                            "tool results require user role",
                        ));
                    }
                    let call_id = calls
                        .get(&result.tool_use_id)
                        .cloned()
                        .or_else(|| {
                            flow.lookup_source(
                                &SourceIdentity::new(
                                    crate::Dialect::Claude,
                                    Some(result.tool_use_id),
                                    0,
                                ),
                                IdentityRole::ToolCall,
                            )
                            .map(|v| v.emitted_id)
                        })
                        .ok_or_else(|| {
                            TransformError::missing_metadata("tool_result call binding")
                        })?;
                    let mut output = super::media::result_to_responses(
                        result
                            .content
                            .unwrap_or(c::ToolResultContent::Text(String::new())),
                    )?;
                    if result.is_error == Some(true) {
                        match &mut output {
                            r::FunctionOutput::Text(text) => {
                                *text = format!("Tool error:\n{text}");
                                report.changed(
                                    "tool_result.is_error",
                                    "error status represented in text",
                                );
                            }
                            r::FunctionOutput::Content(parts) => parts.insert(
                                0,
                                r::FunctionOutputContent::Text(
                                    r::FunctionOutputText::builder(
                                        r::ResponseInputTextType::ResponseInputText,
                                        "Tool error:".into(),
                                    )
                                    .build(),
                                ),
                            ),
                        }
                    }
                    out.push(r::InputItem::FunctionCallOutput(
                        r::FunctionCallOutput::builder(
                            r::FunctionCallOutputType::FunctionCallOutput,
                            call_id,
                            output,
                        )
                        .build(),
                    ));
                }
                c::ContentBlock::McpToolUse(call) => {
                    if call.id.is_empty() || !used.insert(call.id.clone()) {
                        return Err(TransformError::shape(
                            "mcp.id",
                            "empty or duplicate MCP call",
                        ));
                    }
                    let id = flow
                        .resolve_as(
                            IdentityRole::ToolCall,
                            IdentityRole::ToolCall,
                            SourceIdentity::new(
                                crate::Dialect::Claude,
                                Some(call.id.clone()),
                                ordinal,
                            ),
                            policy,
                        )
                        .map_err(|e| TransformError::shape("mcp.identity", e.to_string()))?
                        .emitted_id;
                    ordinal += 1;
                    mcp.insert(call.id, out.len());
                    out.push(r::InputItem::McpCall(
                        r::McpCall::builder(
                            r::McpCallType::McpCall,
                            id,
                            serde_json::to_string(&call.input)?,
                            call.name,
                            call.server_name,
                        )
                        .build(),
                    ));
                }
                c::ContentBlock::McpToolResult(result) => {
                    let index = mcp.remove(&result.tool_use_id).ok_or_else(|| {
                        TransformError::missing_metadata("MCP result paired call")
                    })?;
                    let r::InputItem::McpCall(call) = &mut out[index] else {
                        unreachable!("tracked MCP position")
                    };
                    let text = result.content.map(|content| match content {
                        c::McpResultContent::Text(text) => text,
                        c::McpResultContent::Blocks(blocks) => blocks
                            .into_iter()
                            .map(|v| v.text)
                            .collect::<Vec<_>>()
                            .join(""),
                    });
                    if result.is_error == Some(true) {
                        call.error =
                            Some(Some(text.ok_or_else(|| {
                                TransformError::missing_metadata("MCP error text")
                            })?));
                        call.status = Some(r::McpCallStatus::Failed);
                    } else {
                        call.output = text.map(Some);
                        call.status = Some(r::McpCallStatus::Completed);
                    }
                }
                c::ContentBlock::Thinking(thinking) => {
                    let id = flow
                        .resolve_as(
                            IdentityRole::OutputItem(OutputItemKind::Reasoning),
                            IdentityRole::OutputItem(OutputItemKind::Reasoning),
                            SourceIdentity::new(crate::Dialect::Claude, None, ordinal),
                            policy,
                        )
                        .map_err(|e| TransformError::shape("identity", e.to_string()))?
                        .emitted_id;
                    ordinal += 1;
                    let mut item = r::ReasoningItem::builder(
                        r::ReasoningItemType::ReasoningItem,
                        id,
                        Vec::new(),
                    )
                    .build();
                    item.content = Some(vec![
                        r::ReasoningContent::builder(
                            r::ReasoningTextType::ReasoningText,
                            thinking.thinking,
                        )
                        .build(),
                    ]);
                    report.omitted("thinking.signature","Claude signature must stay in host replay state, never Responses encrypted_content");
                    out.push(r::InputItem::Reasoning(item));
                }
                c::ContentBlock::RedactedThinking(_) => {
                    return Err(TransformError::missing_metadata(
                        "redacted Claude reasoning needs native replay state adapter",
                    ));
                }
                c::ContentBlock::ServerToolUse(_)
                | c::ContentBlock::SearchResult(_)
                | c::ContentBlock::WebSearchToolResult(_)
                | c::ContentBlock::WebFetchToolResult(_)
                | c::ContentBlock::AdvisorToolResult(_)
                | c::ContentBlock::CodeExecutionToolResult(_)
                | c::ContentBlock::BashCodeExecutionToolResult(_)
                | c::ContentBlock::TextEditorCodeExecutionToolResult(_)
                | c::ContentBlock::ToolSearchToolResult(_)
                | c::ContentBlock::ContainerUpload(_)
                | c::ContentBlock::Compaction(_)
                | c::ContentBlock::MidConversationSystem(_)
                | c::ContentBlock::ToolAddition(_)
                | c::ContentBlock::ToolRemoval(_)
                | c::ContentBlock::Fallback(_) => {
                    continue;
                }
            }
        }
    }
    Ok(out)
}
