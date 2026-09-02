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
    let mut extra = Default::default();
    crate::transform::common::preserve_claude_input_transformations(
        &mut extra,
        input.input_transformations,
    );

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
        extra,
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
                if block
                    .citations
                    .as_ref()
                    .is_some_and(|values| !values.is_empty())
                {
                    crate::transform::context::report_unsupported(
                        "content[].text.citations",
                        "Claude citations identify source spans, while OpenAI output annotations require response-text spans",
                    );
                }
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
            claude::ContentBlock::WebSearchToolResult(block) => {
                apply_web_search_result(&mut output, block)
            }
            claude::ContentBlock::WebFetchToolResult(block) => {
                apply_web_fetch_result(&mut output, block)
            }
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

fn apply_web_search_result(
    output: &mut [openai::ResponseOutputItem],
    block: claude::ResponseWebSearchToolResultBlock,
) {
    let Some(item) = find_web_search_call(output, &block.tool_use_id) else {
        crate::transform::context::report_unsupported(
            "content[].web_search_tool_result",
            "OpenAI Responses cannot represent a detached web search result without its matching call",
        );
        return;
    };
    apply_web_search_result_to_item(item, block.content);
}

pub(super) fn apply_web_search_result_to_item(
    item: &mut openai::TypedResponseItem,
    content: claude::ResponseWebSearchToolResultContent,
) {
    let openai::TypedResponseItem::WebSearchCall { action, status, .. } = item else {
        return;
    };
    match content {
        claude::ResponseWebSearchToolResultContent::Results(results) => {
            *status = openai::ResponseWebSearchCallStatus::Completed;
            let sources = results
                .iter()
                .map(|result| {
                    crate::protocol::wire!(openai::WebSearchSource {
                        type_: openai::WebSearchSourceType::Url,
                        url: result.url.clone(),
                        extra: Default::default(),
                    })
                })
                .collect();
            if let openai::WebSearchAction::Search {
                sources: target, ..
            } = action
            {
                *target = Some(sources);
            } else {
                crate::transform::context::report_lossy(
                    "content[].web_search_tool_result",
                    "web search result URLs could not be attached to a non-search OpenAI action",
                );
            }
            if !results.is_empty() {
                crate::transform::context::report_lossy(
                    "content[].web_search_tool_result.content[]",
                    "OpenAI web search sources preserve URLs but not Claude titles, page ages, or encrypted content",
                );
            }
        }
        claude::ResponseWebSearchToolResultContent::Error(_) => {
            *status = openai::ResponseWebSearchCallStatus::Failed;
            crate::transform::context::report_unsupported(
                "content[].web_search_tool_result.error",
                "OpenAI web_search_call can preserve failed status but has no field for the Claude error code",
            );
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn apply_web_fetch_result(
    output: &mut [openai::ResponseOutputItem],
    block: claude::ResponseWebFetchToolResultBlock,
) {
    let Some(item) = find_web_search_call(output, &block.tool_use_id) else {
        crate::transform::context::report_unsupported(
            "content[].web_fetch_tool_result",
            "OpenAI Responses cannot represent a detached web fetch result without its matching call",
        );
        return;
    };
    apply_web_fetch_result_to_item(item, block.content);
}

pub(super) fn apply_web_fetch_result_to_item(
    item: &mut openai::TypedResponseItem,
    content: claude::ResponseWebFetchToolResultContent,
) {
    let openai::TypedResponseItem::WebSearchCall { action, status, .. } = item else {
        return;
    };
    match content {
        claude::ResponseWebFetchToolResultContent::Result(result) => {
            *status = openai::ResponseWebSearchCallStatus::Completed;
            if let openai::WebSearchAction::OpenPage { url } = action {
                *url = Some(result.url);
            } else {
                crate::transform::context::report_lossy(
                    "content[].web_fetch_tool_result",
                    "fetched URL could not be attached to a non-open_page OpenAI action",
                );
            }
            crate::transform::context::report_unsupported(
                "content[].web_fetch_tool_result.content",
                "OpenAI open_page preserves the fetched URL but has no field for fetched document content, title, citations, or retrieval time",
            );
        }
        claude::ResponseWebFetchToolResultContent::Error(_) => {
            *status = openai::ResponseWebSearchCallStatus::Failed;
            crate::transform::context::report_unsupported(
                "content[].web_fetch_tool_result.error",
                "OpenAI web_search_call can preserve failed status but has no field for the Claude fetch error code",
            );
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn find_web_search_call<'a>(
    output: &'a mut [openai::ResponseOutputItem],
    id: &str,
) -> Option<&'a mut openai::TypedResponseItem> {
    let output_item = output.iter_mut().find(|item| {
        matches!(
            &item.0,
            openai::ResponseItem::Typed(openai::TypedResponseItem::WebSearchCall {
                id: item_id,
                ..
            }) if item_id == id
        )
    })?;
    match &mut output_item.0 {
        openai::ResponseItem::Typed(item) => Some(item),
        _ => None,
    }
}

fn typed_output_item(mut item: openai::TypedResponseItem) -> openai::ResponseOutputItem {
    prepare_response_output_item(&mut item);
    openai::ResponseOutputItem::new(openai::ResponseItem::Typed(item))
}

fn reasoning_item(
    thinking: Option<String>,
    encrypted_content: Option<String>,
) -> openai::ResponseOutputItem {
    let id_source = encrypted_content
        .as_deref()
        .or(thinking.as_deref())
        .unwrap_or_default();
    openai::ResponseOutputItem::new(openai::ResponseItem::Typed(
        openai::TypedResponseItem::Reasoning {
            id: Some(common::response_reasoning_item_id(id_source)),
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        )
    }

    #[test]
    fn attaches_claude_web_search_results_to_the_matching_call() {
        let input = serde_json::from_value(json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "server_tool_use",
                    "id": "srv_search",
                    "name": "web_search",
                    "input": {"query": "gproxy"}
                },
                {
                    "type": "web_search_tool_result",
                    "tool_use_id": "srv_search",
                    "content": [{
                        "type": "web_search_result",
                        "url": "https://example.com/result",
                        "title": "Result",
                        "encrypted_content": "ciphertext",
                        "page_age": "today"
                    }]
                },
                {
                    "type": "server_tool_use",
                    "id": "srv_fetch",
                    "name": "web_fetch",
                    "input": {"url": "https://example.com/page"}
                },
                {
                    "type": "web_fetch_tool_result",
                    "tool_use_id": "srv_fetch",
                    "content": {
                        "type": "web_fetch_result",
                        "url": "https://example.com/page",
                        "retrieved_at": "2026-08-15T00:00:00Z",
                        "content": {
                            "type": "document",
                            "title": "Page",
                            "citations": {"enabled": true},
                            "source": {
                                "type": "text",
                                "media_type": "text/plain",
                                "data": "page body"
                            }
                        }
                    }
                }
            ],
            "model": "claude-sonnet-4-6",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }))
        .unwrap();
        let ctx = ctx();

        let output = ctx.scope(|| response(input, &ctx)).unwrap();
        let value = serde_json::to_value(output).unwrap();
        assert_eq!(value["output"][0]["type"], "web_search_call");
        assert_eq!(value["output"][0]["status"], "completed");
        assert_eq!(
            value["output"][0]["action"]["sources"][0]["url"],
            "https://example.com/result"
        );
        assert_eq!(value["output"][1]["type"], "web_search_call");
        assert_eq!(value["output"][1]["action"]["type"], "open_page");
        assert_eq!(
            value["output"][1]["action"]["url"],
            "https://example.com/page"
        );
        assert!(ctx.diagnostics().iter().any(|diagnostic| {
            diagnostic.field == "content[].web_search_tool_result.content[]"
        }));
        assert!(
            ctx.diagnostics().iter().any(|diagnostic| {
                diagnostic.field == "content[].web_fetch_tool_result.content"
            })
        );
    }
}
