use crate::{
    Dialect,
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole, OutputItemKind, SourceIdentity, TargetIdPolicy},
    },
    wire::{
        claude::{content as cc, generate_content as c},
        openai::responses as r,
    },
};

pub(super) fn id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    dialect: Dialect,
    from: IdentityRole,
    to: IdentityRole,
    source: Option<String>,
    index: u64,
) -> Result<String, TransformError> {
    if source.as_ref().is_some_and(String::is_empty) {
        return Err(TransformError::invalid_result(
            "identity",
            "empty native identity",
        ));
    }
    flow.resolve_as(
        from,
        to,
        SourceIdentity::new(dialect, source, index),
        policy,
    )
    .map(|h| h.emitted_id)
    .map_err(|e| TransformError::shape("identity", e.to_string()))
}

pub(super) fn to_responses(
    content: Vec<c::ResponseContentBlock>,
    incomplete: bool,
    refusal: bool,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
    bindings: &crate::transform::generate::client_tools::Bindings,
) -> Result<Vec<r::ResponseOutputItem>, TransformError> {
    let mut output = Vec::new();
    let mut calls = std::collections::BTreeSet::new();
    let mut mcp_calls = std::collections::BTreeMap::new();
    for (index, block) in content.into_iter().enumerate() {
        let index = index as u64;
        let item = match block {
            c::ResponseContentBlock::Text(block) => {
                if block
                    .citations
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|v| !v.is_empty())
                {
                    report.omitted("content.citations","Claude source-document offsets and encrypted citation indexes need explicit target citation facts");
                }
                let part = if refusal {
                    r::OutputContent::Refusal(r::ResponseOutputRefusal {
                        type_: r::ResponseOutputRefusalType::ResponseOutputRefusal,
                        refusal: block.text,
                        rest: Default::default(),
                    })
                } else {
                    r::OutputContent::Text(r::ResponseOutputText {
                        type_: r::ResponseOutputTextType::ResponseOutputText,
                        text: block.text,
                        annotations: Vec::new(),
                        logprobs: Vec::new(),
                        rest: Default::default(),
                    })
                };
                r::ResponseOutputItem::Message(r::ResponseOutputMessage {
                    id: id(
                        flow,
                        policy,
                        Dialect::Claude,
                        IdentityRole::Message,
                        IdentityRole::OutputItem(OutputItemKind::Message),
                        None,
                        index,
                    )?,
                    content: vec![part],
                    role: r::OutputMessageRole::Assistant,
                    status: if incomplete {
                        r::OutputMessageStatus::Incomplete
                    } else {
                        r::OutputMessageStatus::Completed
                    },
                    phase: None,
                    type_: r::MessageType::Message,
                    rest: Default::default(),
                })
            }
            c::ResponseContentBlock::ToolUse(block) => {
                if block.name.is_empty() || !calls.insert(block.id.clone()) {
                    return Err(TransformError::invalid_result(
                        "content.tool_use",
                        "empty name or duplicate call ID",
                    ));
                }
                let call_id = id(
                    flow,
                    policy,
                    Dialect::Claude,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    Some(block.id.clone()),
                    index,
                )?;
                let item_id = id(
                    flow,
                    policy,
                    Dialect::Claude,
                    IdentityRole::ToolCall,
                    IdentityRole::OutputItem(bindings.kind(&block.name)),
                    Some(block.id),
                    index,
                )?;
                let caller = block
                    .caller
                    .map(|v| {
                        v.map(|v| match v {
                            cc::Caller::Direct(_) => Ok(r::Caller::Direct(r::DirectCaller {
                                rest: Default::default(),
                            })),
                            cc::Caller::Server(_) | cc::Caller::Server20260120(_) => {
                                Err(TransformError::unsupported(
                                    "tool_use.caller",
                                    "server program identity requires invocation mapping",
                                ))
                            }
                        })
                        .transpose()
                    })
                    .map(crate::transform::optional)
                    .transpose()?
                    .flatten();
                bindings.restore(r::FunctionCall {
                    type_: r::FunctionCallType::FunctionCall,
                    arguments: serde_json::to_string(&block.input)?,
                    call_id,
                    name: block.name,
                    id: Some(item_id),
                    namespace: None,
                    caller,
                    status: Some(if incomplete {
                        r::ItemStatus::Incomplete
                    } else {
                        r::ItemStatus::Completed
                    }),
                    rest: Default::default(),
                })?
            }
            c::ResponseContentBlock::Thinking(block) => {
                report.omitted("content.thinking.signature","Claude signature must stay bound to its original model and upstream; it is not Responses encrypted_content");
                r::ResponseOutputItem::Reasoning(r::ReasoningItem {
                    type_: r::ReasoningItemType::ReasoningItem,
                    id: id(
                        flow,
                        policy,
                        Dialect::Claude,
                        IdentityRole::Message,
                        IdentityRole::OutputItem(OutputItemKind::Reasoning),
                        None,
                        index,
                    )?,
                    summary: Vec::new(),
                    content: Some(vec![r::ReasoningContent {
                        type_: r::ReasoningTextType::ReasoningText,
                        text: block.thinking,
                        rest: Default::default(),
                    }]),
                    status: Some(if incomplete {
                        r::ReasoningStatus::Incomplete
                    } else {
                        r::ReasoningStatus::Completed
                    }),
                    encrypted_content: None,
                    rest: Default::default(),
                })
            }
            c::ResponseContentBlock::RedactedThinking(_) => {
                report.omitted("content.redacted_thinking","opaque Claude reasoning requires original-origin state retention; Responses cannot consume it");
                continue;
            }
            c::ResponseContentBlock::McpToolUse(block) => {
                if mcp_calls.insert(block.id.clone(), output.len()).is_some()
                    || !calls.insert(block.id.clone())
                {
                    return Err(TransformError::invalid_result(
                        "mcp_tool_use.id",
                        "duplicate native call ID",
                    ));
                }
                r::ResponseOutputItem::McpCall(super::mcp::call(block, index, flow, policy)?)
            }
            c::ResponseContentBlock::McpToolResult(block) => {
                let position = *mcp_calls
                    .get(&block.tool_use_id)
                    .ok_or_else(|| TransformError::missing_metadata("mcp_tool_result.call"))?;
                let Some(r::ResponseOutputItem::McpCall(call)) = output.get_mut(position) else {
                    unreachable!("registered MCP call position")
                };
                super::mcp::result(block, call, report)?;
                continue;
            }
            c::ResponseContentBlock::ServerToolUse(_)
            | c::ResponseContentBlock::WebSearchToolResult(_)
            | c::ResponseContentBlock::WebFetchToolResult(_)
            | c::ResponseContentBlock::AdvisorToolResult(_)
            | c::ResponseContentBlock::CodeExecutionToolResult(_)
            | c::ResponseContentBlock::BashCodeExecutionToolResult(_)
            | c::ResponseContentBlock::TextEditorCodeExecutionToolResult(_)
            | c::ResponseContentBlock::ToolSearchToolResult(_)
            | c::ResponseContentBlock::ContainerUpload(_)
            | c::ResponseContentBlock::Compaction(_)
            | c::ResponseContentBlock::Fallback(_) => {
                continue;
            }
        };
        output.push(item);
    }
    Ok(output)
}

pub(super) struct ClaudeContent {
    pub blocks: Vec<c::ResponseContentBlock>,
    pub tools: bool,
    pub refusal: bool,
}

pub(super) struct Restoration<'a> {
    pub model: &'a str,
    pub context: Option<&'a mut super::super::request::ClaudeRequestContext>,
}

pub(super) fn to_claude(
    output: Vec<r::ResponseOutputItem>,
    completed: bool,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
    mut restoration: Restoration<'_>,
) -> Result<ClaudeContent, TransformError> {
    let mut result = ClaudeContent {
        blocks: Vec::new(),
        tools: false,
        refusal: false,
    };
    let mut calls = std::collections::BTreeSet::new();
    for (index, item) in output.into_iter().enumerate() {
        match item {
            r::ResponseOutputItem::Message(message) => {
                match message.status {
                    r::OutputMessageStatus::InProgress => {
                        return Err(TransformError::invalid_result(
                            "output.status",
                            "message is still in progress",
                        ));
                    }
                    r::OutputMessageStatus::Incomplete if completed => {
                        return Err(TransformError::invalid_result(
                            "output.status",
                            "completed response contains incomplete message",
                        ));
                    }
                    r::OutputMessageStatus::Completed | r::OutputMessageStatus::Incomplete => {}
                }
                if message.phase.is_some() {
                    report.omitted("output.phase", "Claude has no phase field");
                }
                for part in message.content {
                    let text = match part {
                        r::OutputContent::Text(text) => {
                            if !text.annotations.is_empty() {
                                report.omitted("output.annotations","Claude citations require original document or encrypted-index facts");
                            }
                            if !text.logprobs.is_empty() {
                                report.omitted("output.logprobs", "Claude has no logprob field");
                            }
                            text.text
                        }
                        r::OutputContent::Refusal(refusal) => {
                            result.refusal = true;
                            refusal.refusal
                        }
                    };
                    result
                        .blocks
                        .push(c::ResponseContentBlock::Text(c::ResponseTextBlock {
                            type_: c::ResponseTextBlockType::Tag,
                            text,
                            citations: None,
                            rest: Default::default(),
                        }));
                }
            }
            r::ResponseOutputItem::FunctionCall(call) => {
                if call.name.is_empty() || !calls.insert(call.call_id.clone()) {
                    return Err(TransformError::invalid_result(
                        "output.function_call",
                        "empty name or duplicate call ID",
                    ));
                }
                if call.namespace.is_some() {
                    continue;
                }
                match call.status {
                    Some(r::ItemStatus::InProgress) => {
                        return Err(TransformError::invalid_result(
                            "output.status",
                            "tool call is in progress",
                        ));
                    }
                    Some(r::ItemStatus::Incomplete) if completed => {
                        return Err(TransformError::invalid_result(
                            "output.status",
                            "completed response contains incomplete call",
                        ));
                    }
                    Some(r::ItemStatus::Completed | r::ItemStatus::Incomplete) | None => {}
                }
                let arguments: serde_json::Value =
                    serde_json::from_str(&call.arguments).map_err(|e| {
                        TransformError::invalid_result("function_call.arguments", e.to_string())
                    })?;
                let serde_json::Value::Object(input) = arguments else {
                    continue;
                };
                let caller = call
                    .caller
                    .map(|v| {
                        v.map(|v| match v {
                            r::Caller::Direct(_) => Ok(cc::Caller::Direct(cc::DirectCaller {
                                rest: Default::default(),
                            })),
                            r::Caller::Program(_) => Err(TransformError::unsupported(
                                "function_call.caller",
                                "program identity requires invocation mapping",
                            )),
                        })
                        .transpose()
                    })
                    .map(crate::transform::optional)
                    .transpose()?
                    .flatten();
                let id = id(
                    flow,
                    policy,
                    Dialect::OpenAi,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    Some(call.call_id),
                    index as u64,
                )?;
                result
                    .blocks
                    .push(c::ResponseContentBlock::ToolUse(c::ResponseToolUseBlock {
                        type_: c::ResponseToolUseBlockType::Tag,
                        id,
                        input,
                        name: call.name,
                        caller,
                        rest: Default::default(),
                    }));
                result.tools = true;
            }
            r::ResponseOutputItem::Reasoning(reasoning) => {
                if matches!(reasoning.status, Some(r::ReasoningStatus::InProgress))
                    || completed && matches!(reasoning.status, Some(r::ReasoningStatus::Incomplete))
                {
                    return Err(TransformError::invalid_result(
                        "reasoning.status",
                        "nonterminal reasoning conflicts with response status",
                    ));
                }
                if let Some(context) = restoration.context.as_deref_mut() {
                    let block = super::super::request::restore_reasoning(
                        reasoning,
                        restoration.model,
                        context,
                    )?;
                    result.blocks.push(c::ResponseContentBlock::Thinking(block));
                } else {
                    report.omitted("output.reasoning","Claude thinking requires a valid original-origin signature; Responses reasoning and ciphertext cannot invent one");
                }
            }
            r::ResponseOutputItem::McpCall(call) => {
                if !calls.insert(call.id.clone()) {
                    return Err(TransformError::invalid_result(
                        "mcp_call.id",
                        "duplicate call identity",
                    ));
                }
                result.blocks.extend(super::mcp::to_claude(
                    call,
                    index as u64,
                    flow,
                    policy,
                    report,
                )?);
            }
            r::ResponseOutputItem::FunctionCallOutput(_)
            | r::ResponseOutputItem::FileSearchCall(_)
            | r::ResponseOutputItem::WebSearchCall(_)
            | r::ResponseOutputItem::ComputerCall(_)
            | r::ResponseOutputItem::ComputerCallOutput(_)
            | r::ResponseOutputItem::Program(_)
            | r::ResponseOutputItem::ProgramOutput(_)
            | r::ResponseOutputItem::ToolSearchCall(_)
            | r::ResponseOutputItem::ToolSearchOutput(_)
            | r::ResponseOutputItem::AdditionalTools(_)
            | r::ResponseOutputItem::Compaction(_)
            | r::ResponseOutputItem::ImageGenerationCall(_)
            | r::ResponseOutputItem::CodeInterpreterCall(_)
            | r::ResponseOutputItem::LocalShellCall(_)
            | r::ResponseOutputItem::LocalShellCallOutput(_)
            | r::ResponseOutputItem::ShellCall(_)
            | r::ResponseOutputItem::ShellCallOutput(_)
            | r::ResponseOutputItem::ApplyPatchCall(_)
            | r::ResponseOutputItem::ApplyPatchCallOutput(_)
            | r::ResponseOutputItem::McpListTools(_)
            | r::ResponseOutputItem::McpApprovalRequest(_)
            | r::ResponseOutputItem::McpApprovalResponse(_)
            | r::ResponseOutputItem::CustomToolCall(_)
            | r::ResponseOutputItem::CustomToolCallOutput(_) => {
                continue;
            }
        }
    }
    Ok(result)
}
