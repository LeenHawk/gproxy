use super::annotations;
use crate::{
    Dialect,
    transform::{
        Report, TransformError, TransformErrorKind,
        identity::{IdentityFlow, IdentityRole, OutputItemKind, SourceIdentity, TargetIdPolicy},
    },
    wire::openai::{
        chat::{content as cc, response as c},
        responses::{input as i, response as r},
    },
};

pub(super) fn identity(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    source: Dialect,
    from: IdentityRole,
    to: IdentityRole,
    id: Option<String>,
    index: u64,
) -> Result<String, TransformError> {
    flow.resolve_as(from, to, SourceIdentity::new(source, id, index), policy)
        .map(|h| h.emitted_id)
        .map_err(|e| {
            TransformError::with_source(
                TransformErrorKind::Conflict,
                "response.identity",
                "response identity could not be bound",
                e,
            )
        })
}

pub(super) fn to_responses(
    message: c::ResponseMessage,
    logs: Option<c::Logprobs>,
    incomplete: bool,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
    bindings: &super::super::client_tools::Bindings,
) -> Result<Vec<r::ResponseOutputItem>, TransformError> {
    let reasoning = crate::wire::openai::chat::visible_reasoning(
        &message.reasoning_content,
        &message.reasoning,
        &message.reasoning_details,
    );
    if message.audio.flatten().is_some() {
        return Err(TransformError::unsupported(
            "message.audio",
            "Responses has no native output-audio item on this wire",
        ));
    }
    let mut parts = Vec::new();
    let content_logs = logs.as_ref().and_then(|v| v.content.clone());
    if logs.as_ref().is_some_and(|v| v.refusal.is_some()) {
        report.omitted(
            "logprobs.refusal",
            "Responses refusal blocks have no logprob field",
        );
    }
    if let Some(text) = message.content {
        parts.push(i::OutputContent::Text(i::ResponseOutputText {
            type_: i::ResponseOutputTextType::ResponseOutputText,
            text,
            annotations: annotations::to_responses(message.annotations)?,
            logprobs: annotations::logs_to_responses(content_logs, report)?,
            rest: Default::default(),
        }));
    } else if message.annotations.is_some() || content_logs.is_some() {
        return Err(TransformError::invalid_result(
            "message.content",
            "text annotations or logprobs have no associated text",
        ));
    }
    if let Some(refusal) = message.refusal {
        parts.push(i::OutputContent::Refusal(i::ResponseOutputRefusal {
            type_: i::ResponseOutputRefusalType::ResponseOutputRefusal,
            refusal,
            rest: Default::default(),
        }));
    }
    let mut output = Vec::new();
    if let Some(text) = reasoning {
        let id = identity(
            flow,
            policy,
            Dialect::OpenAiChat,
            IdentityRole::OutputItem(OutputItemKind::Reasoning),
            IdentityRole::OutputItem(OutputItemKind::Reasoning),
            None,
            0,
        )?;
        let mut item =
            i::ReasoningItem::builder(i::ReasoningItemType::ReasoningItem, id, Vec::new()).build();
        item.content = Some(vec![
            i::ReasoningContent::builder(i::ReasoningTextType::ReasoningText, text).build(),
        ]);
        item.status = Some(if incomplete {
            i::ReasoningStatus::Incomplete
        } else {
            i::ReasoningStatus::Completed
        });
        output.push(r::ResponseOutputItem::Reasoning(item));
    }
    if !parts.is_empty() {
        let id = identity(
            flow,
            policy,
            Dialect::OpenAiChat,
            IdentityRole::Message,
            IdentityRole::OutputItem(OutputItemKind::Message),
            None,
            0,
        )?;
        output.push(r::ResponseOutputItem::Message(i::ResponseOutputMessage {
            id,
            content: parts,
            role: i::OutputMessageRole::Assistant,
            status: if incomplete {
                i::OutputMessageStatus::Incomplete
            } else {
                i::OutputMessageStatus::Completed
            },
            phase: None,
            type_: i::MessageType::Message,
            rest: Default::default(),
        }));
    }
    let calls = message.tool_calls.unwrap_or_default();
    if let Some(call) = message.function_call {
        if !calls.is_empty() {
            return Err(TransformError::invalid_result(
                "message.function_call",
                "legacy and current tool calls coexist ambiguously",
            ));
        }
        let call_id = identity(
            flow,
            policy,
            Dialect::OpenAiChat,
            IdentityRole::ToolCall,
            IdentityRole::ToolCall,
            None,
            0,
        )?;
        let id = identity(
            flow,
            policy,
            Dialect::OpenAiChat,
            IdentityRole::ToolCall,
            IdentityRole::OutputItem(OutputItemKind::FunctionCall),
            None,
            0,
        )?;
        output.push(r::ResponseOutputItem::FunctionCall(i::FunctionCall {
            type_: i::FunctionCallType::FunctionCall,
            arguments: call.arguments,
            call_id,
            name: call.name,
            id: Some(id),
            namespace: None,
            caller: None,
            status: Some(if incomplete {
                i::ItemStatus::Incomplete
            } else {
                i::ItemStatus::Completed
            }),
            rest: Default::default(),
        }));
        report.changed(
            "message.function_call",
            "legacy call receives an invocation-scoped call ID",
        );
    }
    for (index, call) in calls.into_iter().enumerate() {
        let index = u64::try_from(index)
            .map_err(|_| TransformError::invalid_result("tool_calls", "too many tool calls"))?;
        match call {
            cc::MessageToolCall::Function(call) => {
                let id = identity(
                    flow,
                    policy,
                    Dialect::OpenAiChat,
                    IdentityRole::ToolCall,
                    IdentityRole::OutputItem(bindings.kind(&call.function.name)),
                    Some(call.id.clone()),
                    index,
                )?;
                let call_id = identity(
                    flow,
                    policy,
                    Dialect::OpenAiChat,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    Some(call.id),
                    index,
                )?;
                output.push(bindings.restore(i::FunctionCall {
                    type_: i::FunctionCallType::FunctionCall,
                    arguments: call.function.arguments,
                    call_id,
                    name: call.function.name,
                    id: Some(id),
                    namespace: None,
                    caller: None,
                    status: Some(if incomplete {
                        i::ItemStatus::Incomplete
                    } else {
                        i::ItemStatus::Completed
                    }),
                    rest: Default::default(),
                })?);
            }
            cc::MessageToolCall::Custom(call) => {
                let id = identity(
                    flow,
                    policy,
                    Dialect::OpenAiChat,
                    IdentityRole::ToolCall,
                    IdentityRole::OutputItem(OutputItemKind::CustomToolCall),
                    Some(call.id.clone()),
                    index,
                )?;
                let call_id = identity(
                    flow,
                    policy,
                    Dialect::OpenAiChat,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    Some(call.id),
                    index,
                )?;
                output.push(r::ResponseOutputItem::CustomToolCall(i::CustomToolCall {
                    type_: i::CustomToolCallType::CustomToolCall,
                    call_id,
                    input: call.custom.input,
                    name: call.custom.name,
                    id: Some(id),
                    caller: None,
                    namespace: None,
                    rest: Default::default(),
                }));
            }
        }
    }
    Ok(output)
}

pub(super) struct ChatOutput {
    pub message: c::ResponseMessage,
    pub logprobs: Option<c::Logprobs>,
    pub has_tools: bool,
}

pub(super) fn to_chat(
    output: Vec<r::ResponseOutputItem>,
    completed: bool,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
) -> Result<ChatOutput, TransformError> {
    let mut text = String::new();
    let mut reasoning = Vec::new();
    let mut has_text = false;
    let mut refusal = String::new();
    let mut has_refusal = false;
    let mut annotations_out = Vec::new();
    let mut logs = Vec::new();
    let mut calls = Vec::new();
    for (index, item) in output.into_iter().enumerate() {
        match item {
            r::ResponseOutputItem::Message(message) => {
                match message.status {
                    i::OutputMessageStatus::InProgress => {
                        return Err(TransformError::invalid_result(
                            "output.message.status",
                            "message is still in progress",
                        ));
                    }
                    i::OutputMessageStatus::Incomplete if completed => {
                        return Err(TransformError::invalid_result(
                            "output.message.status",
                            "completed response contains incomplete message",
                        ));
                    }
                    i::OutputMessageStatus::Completed | i::OutputMessageStatus::Incomplete => {}
                }
                if message.phase.is_some() {
                    report.omitted("output.message.phase", "Chat has no message phase field");
                }
                for part in message.content {
                    match part {
                        i::OutputContent::Text(part) => {
                            let offset = i64::try_from(text.chars().count()).map_err(|_| {
                                TransformError::invalid_result("output.text", "text index overflow")
                            })?;
                            annotations_out.extend(annotations::to_chat(
                                part.annotations,
                                offset,
                                report,
                            )?);
                            logs.extend(annotations::logs_to_chat(part.logprobs)?);
                            text.push_str(&part.text);
                            has_text = true;
                        }
                        i::OutputContent::Refusal(part) => {
                            refusal.push_str(&part.refusal);
                            has_refusal = true;
                        }
                    }
                }
            }
            r::ResponseOutputItem::FunctionCall(call) => {
                if matches!(call.status, Some(i::ItemStatus::InProgress))
                    || (completed && matches!(call.status, Some(i::ItemStatus::Incomplete)))
                {
                    return Err(TransformError::invalid_result(
                        "output.function_call.status",
                        "tool call has not reached the response terminal state",
                    ));
                }
                if call.namespace.is_some()
                    || call
                        .caller
                        .flatten()
                        .is_some_and(|caller| matches!(caller, i::Caller::Program(_)))
                {
                    continue;
                }
                let id = identity(
                    flow,
                    policy,
                    Dialect::OpenAi,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    Some(call.call_id),
                    index as u64,
                )?;
                calls.push(cc::MessageToolCall::Function(cc::ChatToolCall {
                    id,
                    function: cc::FunctionCall {
                        name: call.name,
                        arguments: call.arguments,
                        rest: Default::default(),
                    },
                    type_: cc::ChatToolCallType::Function,
                    rest: Default::default(),
                }));
            }
            r::ResponseOutputItem::CustomToolCall(call) => {
                if call.namespace.is_some()
                    || call
                        .caller
                        .flatten()
                        .is_some_and(|caller| matches!(caller, i::Caller::Program(_)))
                {
                    continue;
                }
                let id = identity(
                    flow,
                    policy,
                    Dialect::OpenAi,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    Some(call.call_id),
                    index as u64,
                )?;
                calls.push(cc::MessageToolCall::Custom(cc::CustomToolCall {
                    id,
                    custom: cc::CustomCall {
                        input: call.input,
                        name: call.name,
                        rest: Default::default(),
                    },
                    type_: cc::CustomToolCallType::Custom,
                    rest: Default::default(),
                }));
            }
            r::ResponseOutputItem::Reasoning(item) => {
                reasoning.extend(item.summary.into_iter().map(|part| part.text));
                reasoning.extend(item.content.into_iter().flatten().map(|part| part.text));
                if item.encrypted_content.flatten().is_some() {
                    report.omitted(
                        "output.reasoning.encrypted_content",
                        "opaque replay requires original-bound state",
                    );
                }
            }
            r::ResponseOutputItem::FileSearchCall(_)
            | r::ResponseOutputItem::FunctionCallOutput(_)
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
            | r::ResponseOutputItem::McpCall(_)
            | r::ResponseOutputItem::McpListTools(_)
            | r::ResponseOutputItem::McpApprovalRequest(_)
            | r::ResponseOutputItem::McpApprovalResponse(_)
            | r::ResponseOutputItem::CustomToolCallOutput(_) => continue,
        }
    }
    let has_tools = !calls.is_empty();
    Ok(ChatOutput {
        has_tools,
        logprobs: (!logs.is_empty()).then_some(c::Logprobs {
            content: Some(logs),
            refusal: None,
            rest: Default::default(),
        }),
        message: c::ResponseMessage {
            reasoning_details: None,
            reasoning_content: None,
            reasoning: None,
            content: has_text.then_some(text),
            refusal: has_refusal.then_some(refusal),
            role: c::ResponseRole::Assistant,
            annotations: (!annotations_out.is_empty()).then_some(annotations_out),
            audio: None,
            function_call: None,
            tool_calls: has_tools.then_some(calls),
            rest: Default::default(),
        },
    })
}
