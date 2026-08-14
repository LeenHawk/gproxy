use crate::protocol::{claude, openai};
use crate::transform::compact::claude_to_openai::{
    apply_patch_result, prepare_response_output_item, server_tool_call, shell_result,
    tool_search_result, typed_tool_call,
};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::usage::claude_usage_to_response;

pub fn response(
    input: claude::CreateMessageResponseBody,
    _: &TransformContext,
) -> Result<openai::ResponseObject, TransformError> {
    let id = input.id.clone();
    let model = common::claude_model_string(input.model).into();
    let service_tier = common::claude_usage_to_openai_service_tier(&input.usage);
    let usage = Some(claude_usage_to_response(input.usage));
    let (output, output_text) = claude_content_to_openai_output(id.clone(), input.content);
    let (status, incomplete_details) = response_status(input.stop_reason);

    Ok(crate::protocol::wire!(openai::ResponseObject {
        id,
        created_at: 0,
        background: None,
        completed_at: matches!(status, openai::ResponseStatus::Completed).then_some(0),
        conversation: None,
        error: None,
        incomplete_details,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(model),
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output,
        output_text,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier,
        status: Some(status),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage,
        user: None,
        extra: Default::default(),
    }))
}

fn claude_content_to_openai_output(
    message_id: String,
    content: Vec<claude::ContentBlock>,
) -> (Vec<openai::ResponseOutputItem>, Option<String>) {
    let mut output = Vec::new();
    let mut text = Vec::new();
    let mut message_parts = Vec::new();

    for block in content {
        match block {
            claude::ContentBlock::Text(block) => {
                text.push(block.text.clone());
                message_parts.push(openai::ResponseMessageOutputContentPart::OutputText {
                    annotations: Vec::new(),
                    logprobs: None,
                    text: block.text,
                    extra: Default::default(),
                });
            }
            claude::ContentBlock::Thinking(block) => {
                output.push(reasoning_item(Some(block.thinking), Some(block.signature)))
            }
            claude::ContentBlock::RedactedThinking(block) => {
                output.push(reasoning_item(None, Some(block.data)))
            }
            claude::ContentBlock::ToolUse(block) => output.push(typed_output_item(
                typed_tool_call(block.id, block.input, block.name).0,
            )),
            claude::ContentBlock::ServerToolUse(block) => output.push(typed_output_item(
                server_tool_call(block.id, block.input, block.name),
            )),
            claude::ContentBlock::BashCodeExecutionToolResult(block) => output.push(
                typed_output_item(shell_result(block.tool_use_id, &block.content)),
            ),
            claude::ContentBlock::TextEditorCodeExecutionToolResult(block) => output.push(
                typed_output_item(apply_patch_result(block.tool_use_id, &block.content)),
            ),
            claude::ContentBlock::ToolSearchToolResult(block) => output.push(typed_output_item(
                tool_search_result(block.tool_use_id, &block.content),
            )),
            _ => {}
        }
    }

    if !message_parts.is_empty() {
        output.push(openai::ResponseOutputItem::new(
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                crate::protocol::wire!(openai::ResponseOutputMessageItem {
                    type_: openai::ResponseMessageItemType::Message,
                    id: message_id,
                    role: openai::ResponseOutputMessageRole::Assistant,
                    content: message_parts,
                    status: openai::ResponseItemLifecycleStatus::Completed,
                    phase: None,
                    extra: Default::default(),
                }),
            )),
        ));
    }
    let output_text = (!text.is_empty()).then(|| text.join(""));
    (output, output_text)
}

fn typed_output_item(mut item: openai::TypedResponseItem) -> openai::ResponseOutputItem {
    prepare_response_output_item(&mut item);
    openai::ResponseOutputItem::new(openai::ResponseItem::Typed(item))
}

fn reasoning_item(
    thinking: Option<String>,
    encrypted_content: Option<String>,
) -> openai::ResponseOutputItem {
    openai::ResponseOutputItem::new(openai::ResponseItem::Typed(
        openai::TypedResponseItem::Reasoning {
            id: Some("reasoning".to_owned()),
            summary: Vec::new(),
            content: thinking.map(|text| {
                vec![crate::protocol::wire!(openai::ResponseReasoningTextPart {
                    text,
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    extra: Default::default(),
                })]
            }),
            encrypted_content,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            extra: Default::default(),
        },
    ))
}

fn response_status(
    reason: claude::StopReason,
) -> (openai::ResponseStatus, Option<openai::IncompleteDetails>) {
    match reason {
        claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        | claude::StopReason::Known(claude::StopReasonKnown::ModelContextWindowExceeded) => (
            openai::ResponseStatus::Incomplete,
            Some(crate::protocol::wire!(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                extra: Default::default(),
            })),
        ),
        claude::StopReason::Known(claude::StopReasonKnown::Refusal) => (
            openai::ResponseStatus::Incomplete,
            Some(crate::protocol::wire!(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                extra: Default::default(),
            })),
        ),
        _ => (openai::ResponseStatus::Completed, None),
    }
}
