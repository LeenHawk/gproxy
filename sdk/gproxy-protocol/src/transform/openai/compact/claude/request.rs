use crate::claude::count_tokens::types as ct;
use crate::claude::create_message::request::{
    ClaudeCreateMessageRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::claude::create_message::types::BetaSystemPrompt;
use crate::openai::compact_response::request::OpenAiCompactRequest;
use crate::openai::count_tokens::types as ot;
use crate::transform::openai::compact::utils::COMPACT_MAX_OUTPUT_TOKENS;
use crate::transform::openai::compact::utils::claude_compact_system_instruction;
use crate::transform::openai::count_tokens::claude::utils::{
    ClaudeToolUseIdMapper, openai_message_content_to_claude, openai_role_to_claude,
    push_message_block,
};
use crate::transform::openai::count_tokens::utils::{
    openai_function_call_output_content_to_text, openai_input_to_items,
    openai_reasoning_summary_to_text,
};
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiCompactRequest> for ClaudeCreateMessageRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiCompactRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let mut messages = Vec::new();
        let mut tool_use_ids = ClaudeToolUseIdMapper::default();

        for item in openai_input_to_items(body.input) {
            match item {
                ot::ResponseInputItem::Message(message) => {
                    messages.push(ct::BetaMessageParam {
                        content: openai_message_content_to_claude(message.content),
                        role: openai_role_to_claude(message.role),
                    });
                }
                ot::ResponseInputItem::OutputMessage(message) => {
                    let text = message
                        .content
                        .into_iter()
                        .map(|part| match part {
                            ot::ResponseOutputContent::Text(text) => text.text,
                            ot::ResponseOutputContent::Refusal(refusal) => refusal.refusal,
                        })
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Text(text),
                            role: ct::BetaMessageRole::Assistant,
                        });
                    }
                }
                ot::ResponseInputItem::FunctionToolCall(tool_call) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_call.call_id);
                    let input = serde_json::from_str::<ct::JsonObject>(&tool_call.arguments)
                        .unwrap_or_default();
                    push_message_block(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                            id: tool_use_id,
                            input,
                            name: tool_call.name,
                            type_: ct::BetaToolUseBlockType::ToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::FunctionCallOutput(tool_result) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_result.call_id);
                    let output_text =
                        openai_function_call_output_content_to_text(&tool_result.output);
                    push_message_block(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content: if output_text.is_empty() {
                                None
                            } else {
                                Some(ct::BetaToolResultBlockParamContent::Text(output_text))
                            },
                            is_error: None,
                        }),
                    );
                }
                ot::ResponseInputItem::ReasoningItem(reasoning) => {
                    let mut thinking = openai_reasoning_summary_to_text(&reasoning.summary);
                    if thinking.is_empty()
                        && let Some(encrypted) = reasoning.encrypted_content
                    {
                        thinking = encrypted;
                    }
                    if !thinking.is_empty()
                        && let Some(signature) = reasoning.id.filter(|id| !id.is_empty())
                    {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Blocks(vec![
                                ct::BetaContentBlockParam::Thinking(ct::BetaThinkingBlockParam {
                                    signature,
                                    thinking,
                                    type_: ct::BetaThinkingBlockType::Thinking,
                                }),
                            ]),
                            role: ct::BetaMessageRole::Assistant,
                        });
                    }
                }
                other => {
                    messages.push(ct::BetaMessageParam {
                        content: ct::BetaMessageContent::Text(format!("{other:?}")),
                        role: ct::BetaMessageRole::User,
                    });
                }
            }
        }

        Ok(ClaudeCreateMessageRequest {
            method: ct::HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                max_tokens: COMPACT_MAX_OUTPUT_TOKENS,
                messages,
                model: ct::Model::Custom(body.model),
                container: None,
                context_management: None,
                inference_geo: None,
                mcp_servers: None,
                metadata: None,
                cache_control: None,
                output_config: None,
                output_format: None,
                service_tier: None,
                speed: None,
                stop_sequences: None,
                stream: None,
                system: Some(BetaSystemPrompt::Text(claude_compact_system_instruction(
                    body.instructions,
                ))),
                temperature: None,
                thinking: None,
                tool_choice: None,
                tools: None,
                top_k: None,
                top_p: None,
            },
        })
    }
}
