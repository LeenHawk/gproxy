use super::ResponseSupplement;
use super::util::parse_result_arguments;
use crate::transform::generate::reasoning_details as rd;
use crate::{
    Rest,
    transform::{Converted, Report, TransformError},
    wire::{claude::generate_content as cg, openai::chat},
};

pub fn claude_response_to_openai(
    input: &cg::GenerateContentResponseBody,
    target_model: impl Into<String>,
    supplement: &ResponseSupplement,
) -> Result<Converted<chat::GenerateContentResponseBody>, TransformError> {
    let mut report = Report::default();
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut details = Vec::new();
    let mut tool_calls = Vec::new();
    for (index, block) in input.content.iter().enumerate() {
        match block {
            cg::ResponseContentBlock::Text(block) => {
                if block.citations.is_some() {
                    report.omitted(
                        "content.text.citations",
                        "Claude citation locations need a target citation adapter",
                    );
                }
                text.push(block.text.clone());
            }
            cg::ResponseContentBlock::ToolUse(block) => {
                tool_calls.push(chat::MessageToolCall::Function(chat::ChatToolCall {
                    id: super::util::response_id(&block.id)?,
                    function: chat::FunctionCall {
                        name: block.name.clone(),
                        arguments: serde_json::to_string(&block.input)?,
                        rest: Rest::new(),
                    },
                    type_: chat::ChatToolCallType::Function,
                    rest: Rest::new(),
                }))
            }
            cg::ResponseContentBlock::Thinking(block) => {
                reasoning.push(block.thinking.clone());
                details.push(rd::from_thinking(block, index as i64));
            }
            cg::ResponseContentBlock::RedactedThinking(block) => {
                details.push(rd::from_redacted(block, index as i64))
            }
            cg::ResponseContentBlock::ServerToolUse(_)
            | cg::ResponseContentBlock::WebSearchToolResult(_)
            | cg::ResponseContentBlock::WebFetchToolResult(_)
            | cg::ResponseContentBlock::AdvisorToolResult(_)
            | cg::ResponseContentBlock::CodeExecutionToolResult(_)
            | cg::ResponseContentBlock::BashCodeExecutionToolResult(_)
            | cg::ResponseContentBlock::TextEditorCodeExecutionToolResult(_)
            | cg::ResponseContentBlock::ToolSearchToolResult(_)
            | cg::ResponseContentBlock::McpToolUse(_)
            | cg::ResponseContentBlock::McpToolResult(_)
            | cg::ResponseContentBlock::ContainerUpload(_)
            | cg::ResponseContentBlock::Compaction(_)
            | cg::ResponseContentBlock::Fallback(_) => {
                continue;
            }
        }
    }
    let finish_reason = match input.stop_reason {
        cg::StopReason::MaxTokens | cg::StopReason::ModelContextWindowExceeded => {
            chat::FinishReason::Length
        }
        cg::StopReason::ToolUse => chat::FinishReason::ToolCalls,
        cg::StopReason::Refusal => chat::FinishReason::ContentFilter,
        cg::StopReason::PauseTurn | cg::StopReason::Compaction => chat::FinishReason::Stop,
        cg::StopReason::EndTurn | cg::StopReason::StopSequence => chat::FinishReason::Stop,
    };
    for (present, field) in [
        (input.container.is_some(), "container"),
        (input.context_management.is_some(), "context_management"),
        (input.diagnostics.is_some(), "diagnostics"),
        (input.stop_details.is_some(), "stop_details"),
        (input.stop_sequence.is_some(), "stop_sequence"),
    ] {
        if present {
            report.omitted(field, "Chat response has no equivalent metadata field");
        }
    }
    let created = supplement
        .created_unix_seconds
        .ok_or_else(|| TransformError::missing_metadata("response.created"))?;
    let usage = super::usage::to_chat(&input.usage, &mut report)?;
    Ok(Converted {
        value: chat::GenerateContentResponseBody {
            id: input.id.clone(),
            choices: vec![chat::Choice {
                finish_reason,
                index: 0,
                logprobs: None,
                message: chat::ResponseMessage {
                    reasoning_details: (!details.is_empty()).then_some(Some(details)),
                    reasoning_content: (!reasoning.is_empty()).then(|| Some(reasoning.join(""))),
                    reasoning: None,
                    content: (!text.is_empty()
                        && !matches!(input.stop_reason, cg::StopReason::Refusal))
                    .then_some(text.join("")),
                    refusal: (matches!(input.stop_reason, cg::StopReason::Refusal))
                        .then_some(text.join("")),
                    role: chat::ResponseRole::Assistant,
                    annotations: None,
                    audio: None,
                    function_call: None,
                    tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
                    rest: Rest::new(),
                },
                rest: Rest::new(),
            }],
            created,
            model: target_model.into(),
            object: chat::CompletionObject::ChatCompletion,
            service_tier: None,
            system_fingerprint: None,
            usage: Some(usage),
            moderation: None,
            rest: Rest::new(),
        },
        report,
    })
}

pub fn openai_response_to_claude(
    input: &chat::GenerateContentResponseBody,
    target_model: impl Into<String>,
    _supplement: &ResponseSupplement,
) -> Result<Converted<cg::GenerateContentResponseBody>, TransformError> {
    let mut report = Report::default();
    let choice = input
        .choices
        .first()
        .ok_or_else(|| TransformError::invalid_result("choices", "Chat response has no choices"))?;
    if input.choices.len() != 1 {
        report.omitted("choices", "target carries the first choice");
    }
    if choice.index != 0 {
        return Err(TransformError::invalid_result(
            "choices.index",
            "single response choice must have index zero",
        ));
    }
    if choice.message.audio.is_some() {
        report.omitted("choices.message.audio", "no target audio block");
    }
    if choice.message.function_call.is_some() {
        report.omitted("message.function_call", "call identity is unavailable");
    }
    for (present, field) in [
        (choice.logprobs.is_some(), "choices.logprobs"),
        (
            choice.message.annotations.is_some(),
            "choices.message.annotations",
        ),
        (input.service_tier.is_some(), "service_tier"),
        (input.system_fingerprint.is_some(), "system_fingerprint"),
        (input.moderation.is_some(), "moderation"),
    ] {
        if present {
            report.omitted(field, "Claude response has no equivalent metadata field");
        }
    }
    let mut content = Vec::new();
    let restored = rd::to_claude(
        choice
            .message
            .reasoning_details
            .as_ref()
            .and_then(Option::as_deref)
            .unwrap_or(&[]),
    );
    let has_restored = !restored.is_empty();
    for block in restored {
        match block {
            crate::wire::claude::content::ContentBlock::Thinking(block) => {
                content.push(cg::ResponseContentBlock::Thinking(block))
            }
            crate::wire::claude::content::ContentBlock::RedactedThinking(block) => {
                content.push(cg::ResponseContentBlock::RedactedThinking(block))
            }
            _ => unreachable!(),
        }
    }
    if !has_restored
        && let Some(text) = chat::visible_reasoning(
            &choice.message.reasoning_content,
            &choice.message.reasoning,
            &choice.message.reasoning_details,
        )
    {
        content.push(cg::ResponseContentBlock::Thinking(
            crate::wire::claude::content::ThinkingBlock::builder(
                crate::wire::claude::content::ThinkingBlockType::Tag,
                String::new(),
                text,
            )
            .build(),
        ));
        report.changed(
            "message.reasoning_content",
            "plain reasoning has no replayable Claude signature",
        );
    }

    if let Some(text) = &choice.message.content {
        content.push(cg::ResponseContentBlock::Text(cg::ResponseTextBlock {
            type_: cg::ResponseTextBlockType::Tag,
            citations: None,
            text: text.clone(),
            rest: Rest::new(),
        }));
    }
    if let Some(refusal) = &choice.message.refusal {
        content.push(cg::ResponseContentBlock::Text(cg::ResponseTextBlock {
            type_: cg::ResponseTextBlockType::Tag,
            citations: None,
            text: refusal.clone(),
            rest: Rest::new(),
        }));
    }
    if let Some(calls) = &choice.message.tool_calls {
        for call in calls {
            if let chat::MessageToolCall::Function(call) = call {
                let input = parse_result_arguments(
                    &call.function.arguments,
                    "choices.message.tool_calls.arguments",
                )?;
                let input = input.as_object().cloned().expect("object checked");
                content.push(cg::ResponseContentBlock::ToolUse(
                    cg::ResponseToolUseBlock {
                        type_: cg::ResponseToolUseBlockType::Tag,
                        id: super::util::response_id(&call.id)?,
                        input,
                        name: call.function.name.clone(),
                        caller: None,
                        rest: Rest::new(),
                    },
                ));
            } else {
                continue;
            }
        }
    }
    let usage = input
        .usage
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("response.usage"))?;
    let stop_reason = match choice.finish_reason {
        chat::FinishReason::Length => cg::StopReason::MaxTokens,
        chat::FinishReason::ToolCalls | chat::FinishReason::FunctionCall => cg::StopReason::ToolUse,
        chat::FinishReason::ContentFilter => cg::StopReason::Refusal,
        chat::FinishReason::Stop if choice.message.refusal.is_some() => cg::StopReason::Refusal,
        chat::FinishReason::Stop => cg::StopReason::EndTurn,
    };
    Ok(Converted {
        value: cg::GenerateContentResponseBody {
            type_: cg::GenerateContentResponseBodyType::Tag,
            id: input.id.clone(),
            container: None,
            content,
            context_management: None,
            diagnostics: None,
            model: target_model.into(),
            role: cg::ResponseRole::Assistant,
            stop_details: None,
            stop_reason,
            stop_sequence: None,
            usage: super::usage::to_claude(usage, &mut report)?,
            rest: Rest::new(),
        },
        report,
    })
}
