use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::tools::{claude_tool_choice_to_responses, claude_tools_to_responses};
use crate::transform::compact::claude_to_openai::claude_messages_to_openai_items;

pub fn request(
    input: claude::CreateMessageRequestBody,
    _: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    let prompt_cache_key = common::claude_prompt_cache_key(&input);
    let effort = input
        .output_config
        .as_ref()
        .and_then(|config| common::claude_effort_to_openai(config.effort.clone()))
        .or_else(|| common::claude_thinking_to_openai(input.thinking.clone()));
    #[allow(deprecated)]
    let format = input
        .output_config
        .as_ref()
        .and_then(|config| config.format.clone())
        .or(input.output_format.clone());
    let text = format.map(|format| {
        crate::protocol::wire!(openai::TextConfig {
            format: common::claude_output_format_to_response(Some(format)),
            verbosity: None,
            extra: Default::default(),
        })
    });
    let tools = claude_tools_to_responses(input.tools.clone(), input.mcp_servers.clone());
    let tool_choice = claude_tool_choice_to_responses(input.tool_choice.clone());
    let mut items = system_items(input.system);
    items.extend(claude_messages_to_openai_items(input.messages));

    Ok(crate::protocol::wire!(openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(items)),
        instructions: None,
        max_output_tokens: Some(u64_to_u32(input.max_tokens)),
        max_tool_calls: None,
        metadata: input.metadata.and_then(|metadata| {
            metadata.user_id.map(|user_id| {
                let mut metadata = openai::Metadata::new();
                metadata.insert("user_id".to_owned(), user_id);
                metadata
            })
        }),
        model: Some(common::claude_model_string(input.model).into()),
        moderation: None,
        multi_agent: None,
        parallel_tool_calls: input
            .tool_choice
            .as_ref()
            .and_then(claude_parallel_tool_calls),
        previous_response_id: None,
        prompt_cache_key: Some(prompt_cache_key),
        prompt_cache_options: Some(common::openai_options_for_claude_root(input.cache_control)),
        prompt_cache_retention: None,
        prompt: None,
        reasoning: effort.map(|effort| crate::protocol::wire!(openai::ReasoningConfig {
            context: None,
            effort: Some(effort),
            mode: None,
            summary: None,
            generate_summary: None,
            extra: Default::default(),
        })),
        safety_identifier: None,
        service_tier: common::claude_speed_to_openai(input.speed)
            .or_else(|| common::claude_service_tier_to_openai(input.service_tier)),
        store: None,
        stream: input.stream,
        stream_options: None,
        temperature: input.temperature,
        text,
        tool_choice,
        tools,
        top_logprobs: None,
        top_p: input.top_p,
        truncation: None,
        user: None,
        extra: Default::default(),
    }))
}

fn system_items(system: Option<claude::SystemPrompt>) -> Vec<openai::ResponseItem> {
    let blocks = match system {
        Some(claude::StringOrArray::String(text)) => vec![(text, None)],
        Some(claude::StringOrArray::Array(blocks)) => blocks
            .into_iter()
            .map(|block| (block.text, block.cache_control))
            .collect(),
        None => Vec::new(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    if blocks.is_empty() {
        return Vec::new();
    }
    vec![openai::ResponseItem::Message(
        openai::ResponseMessageItem::EasyInput(crate::protocol::wire!(
            openai::ResponseEasyInputMessageItem {
                type_: Some(openai::ResponseMessageItemType::Message),
                role: openai::ResponseEasyInputMessageRole::System,
                content: openai::ResponseEasyInputContent::Parts(
                    blocks
                        .into_iter()
                        .map(|(text, cache_control)| {
                            openai::ResponseInputContentPart::InputText {
                                text,
                                prompt_cache_breakpoint: common::openai_breakpoint(cache_control),
                                extra: Default::default(),
                            }
                        })
                        .collect(),
                ),
                phase: None,
                extra: Default::default(),
            }
        )),
    )]
}

fn claude_parallel_tool_calls(choice: &claude::ToolChoice) -> Option<bool> {
    match choice {
        claude::ToolChoice::Auto(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::Any(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::Tool(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::None(_) | claude::ToolChoice::Unknown(_) => None,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
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
    fn preserves_representable_claude_breakpoints_in_responses() {
        let input = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "system": [{"type": "text", "text": "system", "cache_control": {"type": "ephemeral"}}],
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "stable", "cache_control": {"type": "ephemeral"}},
                    {"type": "text", "text": "question"}
                ]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "answer", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        );

        let output = serde_json::to_value(request(input, &ctx).unwrap()).unwrap();
        assert_eq!(
            output["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(
            output["input"][1]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert!(
            output["input"][1]["content"][1]
                .get("prompt_cache_breakpoint")
                .is_none()
        );
        assert_eq!(
            output["input"][2]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(output["input"][1]["content"][0]["type"], "input_text");
        assert_eq!(output["input"][2]["role"], "assistant");
        assert_eq!(output["input"][2]["content"][0]["type"], "output_text");
    }

    #[test]
    fn maps_assistant_history_to_responses_output_text() {
        let input = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "messages": [
                {"role": "assistant", "content": "previous answer"},
                {"role": "user", "content": "continue"}
            ]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        assert_eq!(output["input"][0]["role"], "assistant");
        assert_eq!(output["input"][0]["content"][0]["type"], "output_text");
        assert_eq!(output["input"][0]["content"][0]["text"], "previous answer");
        assert_eq!(output["input"][1]["role"], "user");
        assert_eq!(output["input"][1]["content"][0]["type"], "input_text");
    }

    #[test]
    fn maps_claude_signature_directly_to_encrypted_reasoning() {
        let input = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "messages": [{
                "role": "assistant",
                "content": [{
                    "type": "thinking",
                    "thinking": "hidden",
                    "signature": "ciphertext"
                }]
            }]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        );

        let output = serde_json::to_value(request(input, &ctx).unwrap()).unwrap();
        let reasoning = output["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "reasoning")
            .unwrap();
        assert_eq!(reasoning["encrypted_content"], "ciphertext");
        assert_eq!(reasoning["content"][0]["text"], "hidden");
        assert!(
            reasoning["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("rs_"))
        );
    }

    #[test]
    fn maps_approximate_tools_and_calls_end_to_end() {
        let input = serde_json::from_value(serde_json::json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "tools": [
                {"type": "bash_20250124", "name": "bash"},
                {"type": "text_editor_20250728", "name": "str_replace_based_edit_tool"},
                {"type": "web_fetch_20260209", "name": "web_fetch"},
                {"type": "tool_search_tool_bm25", "name": "tool_search_tool_bm25"}
            ],
            "messages": [
                {"role": "assistant", "content": [{
                    "type": "tool_use", "id": "toolu_shell", "name": "bash",
                    "input": {"command": "pwd"}
                }]},
                {"role": "user", "content": [{
                    "type": "tool_result", "tool_use_id": "toolu_shell", "content": "ok"
                }]},
                {"role": "assistant", "content": [{
                    "type": "server_tool_use", "id": "srv_fetch", "name": "web_fetch",
                    "input": {"url": "https://example.com"}
                }]}
            ]
        }))
        .unwrap();

        let output = serde_json::to_value(request(input, &ctx()).unwrap()).unwrap();
        let tool_types = output["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["type"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            tool_types,
            ["shell", "apply_patch", "web_search", "tool_search"]
        );
        let item_types = output["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["type"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            item_types,
            ["shell_call", "shell_call_output", "web_search_call"]
        );
        assert_eq!(output["input"][2]["action"]["type"], "open_page");
    }
}
