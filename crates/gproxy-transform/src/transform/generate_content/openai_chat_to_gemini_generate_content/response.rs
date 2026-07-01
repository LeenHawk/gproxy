use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn response(
    input: openai::ChatCompletionResponse,
    _: &TransformContext,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    Ok(gemini::GenerateContentResponse {
        candidates: input
            .choices
            .into_iter()
            .map(|choice| gemini::Candidate {
                content: Some(chat_message_to_gemini_content(choice.message)),
                finish_reason: Some(chat_finish_reason_to_gemini(choice.finish_reason)),
                safety_ratings: Vec::new(),
                citation_metadata: None,
                token_count: None,
                grounding_metadata: None,
                avg_logprobs: None,
                logprobs_result: None,
                url_context_metadata: None,
                index: Some(u32_to_i32(choice.index)),
                finish_message: None,
                extra: Default::default(),
            })
            .collect(),
        prompt_feedback: None,
        usage_metadata: completion_usage_to_gemini(input.usage),
        model_version: Some(common::openai_model_string(input.model)),
        response_id: Some(input.id),
        model_status: None,
        extra: Default::default(),
    })
}

fn chat_message_to_gemini_content(message: openai::ChatMessage) -> gemini::Content {
    gemini_content_to_chat_message::chat_message_to_gemini_content(message)
}

fn completion_usage_to_gemini(
    usage: Option<openai::CompletionUsage>,
) -> Option<gemini::UsageMetadata> {
    let usage = usage?;
    Some(gemini::UsageMetadata {
        prompt_token_count: Some(u32_to_i32(usage.prompt_tokens)),
        cached_content_token_count: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .map(u32_to_i32),
        candidates_token_count: Some(u32_to_i32(usage.completion_tokens)),
        thoughts_token_count: usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens)
            .map(u32_to_i32),
        total_token_count: Some(u32_to_i32(usage.total_tokens)),
        tool_use_prompt_token_count: None,
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: None,
        extra: Default::default(),
    })
}

fn chat_finish_reason_to_gemini(reason: openai::ChatFinishReason) -> gemini::FinishReason {
    let known = match reason {
        openai::ChatFinishReason::Stop => gemini::FinishReasonKnown::Stop,
        openai::ChatFinishReason::Length => gemini::FinishReasonKnown::MaxTokens,
        openai::ChatFinishReason::ToolCalls | openai::ChatFinishReason::FunctionCall => {
            gemini::FinishReasonKnown::Stop
        }
        openai::ChatFinishReason::ContentFilter => gemini::FinishReasonKnown::Safety,
    };
    gemini::FinishReason::Known(known)
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

mod gemini_content_to_chat_message {
    use crate::protocol::{gemini, openai};

    pub(super) fn chat_message_to_gemini_content(message: openai::ChatMessage) -> gemini::Content {
        let mut parts = Vec::new();
        if let Some(content) = message.content.filter(|value| !value.is_empty()) {
            parts.push(gemini::Part {
                data: Some(gemini::PartData::Text { text: content }),
                ..Default::default()
            });
        }
        if let Some(refusal) = message.refusal.filter(|value| !value.is_empty()) {
            parts.push(gemini::Part {
                data: Some(gemini::PartData::Text { text: refusal }),
                ..Default::default()
            });
        }
        if let Some(tool_calls) = message.tool_calls {
            for call in tool_calls {
                parts.push(match call {
                    openai::ChatToolCall::Function { id, function, .. } => gemini::Part {
                        data: Some(gemini::PartData::FunctionCall {
                            function_call: gemini::FunctionCall {
                                id: Some(id),
                                name: function.name,
                                args: serde_json::from_str(&function.arguments).ok(),
                                extra: Default::default(),
                            },
                        }),
                        ..Default::default()
                    },
                    openai::ChatToolCall::Custom { id, custom, .. } => gemini::Part {
                        data: Some(gemini::PartData::FunctionCall {
                            function_call: gemini::FunctionCall {
                                id: Some(id),
                                name: custom.name,
                                args: serde_json::from_str(&custom.input).ok(),
                                extra: Default::default(),
                            },
                        }),
                        ..Default::default()
                    },
                });
            }
        }

        gemini::Content {
            parts,
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            extra: Default::default(),
        }
    }
}
