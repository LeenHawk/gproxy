use std::collections::BTreeMap;

use super::utils::{
    gemini_citation_annotations, gemini_grounding_to_web_search_item, gemini_logprobs,
};
use crate::gemini::generate_content::response::GeminiGenerateContentResponse;
use crate::gemini::generate_content::types as gt;
use crate::openai::count_tokens::types as ot;
use crate::openai::create_response::response::{OpenAiCreateResponseResponse, ResponseBody};
use crate::openai::create_response::types as rt;
use crate::openai::types::OpenAiResponseHeaders;
use crate::transform::openai::generate_content::openai_chat_completions::gemini::utils::{
    gemini_function_response_to_text, json_object_to_string, prompt_feedback_refusal_text,
};
use crate::transform::openai::model_list::gemini::utils::{
    openai_error_response_from_gemini, strip_models_prefix,
};
use crate::transform::utils::TransformError;

impl TryFrom<GeminiGenerateContentResponse> for OpenAiCreateResponseResponse {
    type Error = TransformError;

    fn try_from(value: GeminiGenerateContentResponse) -> Result<Self, TransformError> {
        Ok(match value {
            GeminiGenerateContentResponse::Success {
                stats_code,
                headers,
                body,
            } => {
                let response_id = body.response_id.unwrap_or_default();
                let response_model = body
                    .model_version
                    .as_deref()
                    .map(strip_models_prefix)
                    .unwrap_or_default();
                let usage = body.usage_metadata.map(|usage| {
                    let input_tokens = usage
                        .prompt_token_count
                        .unwrap_or(0)
                        .saturating_add(usage.tool_use_prompt_token_count.unwrap_or(0));
                    let cached_tokens = usage.cached_content_token_count.unwrap_or(0);
                    let output_tokens = usage
                        .candidates_token_count
                        .unwrap_or(0)
                        .saturating_add(usage.thoughts_token_count.unwrap_or(0));
                    let total_tokens = usage
                        .total_token_count
                        .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));

                    rt::ResponseUsage {
                        input_tokens,
                        input_tokens_details: rt::ResponseInputTokensDetails { cached_tokens },
                        output_tokens,
                        output_tokens_details: rt::ResponseOutputTokensDetails {
                            reasoning_tokens: usage.thoughts_token_count.unwrap_or(0),
                        },
                        total_tokens,
                    }
                });
                let prompt_feedback = body.prompt_feedback;

                let mut output = Vec::new();
                let mut output_text_parts = Vec::new();
                let mut tool_call_count = 0usize;
                let mut first_finish_reason = None;

                for (candidate_pos, candidate) in
                    body.candidates.unwrap_or_default().into_iter().enumerate()
                {
                    let candidate_index = candidate.index.unwrap_or(candidate_pos as u32);

                    if first_finish_reason.is_none() {
                        first_finish_reason = candidate.finish_reason.clone();
                    }

                    if let Some(web_search_item) = gemini_grounding_to_web_search_item(
                        candidate_index,
                        candidate.grounding_metadata.as_ref(),
                    ) {
                        tool_call_count += 1;
                        output.push(web_search_item);
                    }

                    let annotations =
                        gemini_citation_annotations(candidate.citation_metadata.as_ref());
                    let logprobs = gemini_logprobs(candidate.logprobs_result.as_ref());
                    let mut logprobs_attached = false;
                    let mut message_content = Vec::new();

                    if let Some(content) = candidate.content {
                        for (part_index, part) in content.parts.into_iter().enumerate() {
                            if part.thought.unwrap_or(false) {
                                if let Some(thinking) = part.text
                                    && !thinking.is_empty()
                                {
                                    let reasoning_id =
                                        part.thought_signature.unwrap_or_else(|| {
                                            format!(
                                                "candidate_{candidate_index}_reasoning_{part_index}"
                                            )
                                        });
                                    output.push(rt::ResponseOutputItem::ReasoningItem(
                                            ot::ResponseReasoningItem {
                                                id: Some(reasoning_id),
                                                summary: vec![ot::ResponseSummaryTextContent {
                                                    text: thinking.clone(),
                                                    type_: ot::ResponseSummaryTextContentType::SummaryText,
                                                }],
                                                type_: ot::ResponseReasoningItemType::Reasoning,
                                                content: Some(vec![ot::ResponseReasoningTextContent {
                                                    text: thinking,
                                                    type_: ot::ResponseReasoningTextContentType::ReasoningText,
                                                }]),
                                                encrypted_content: None,
                                                status: Some(ot::ResponseItemStatus::Completed),
                                            },
                                        ));
                                }
                                continue;
                            }

                            if let Some(function_call) = part.function_call {
                                tool_call_count += 1;
                                let call_id = function_call.id.unwrap_or_else(|| {
                                    format!("candidate_{candidate_index}_tool_{part_index}")
                                });
                                output.push(rt::ResponseOutputItem::FunctionToolCall(
                                    ot::ResponseFunctionToolCall {
                                        arguments: function_call
                                            .args
                                            .as_ref()
                                            .map(json_object_to_string)
                                            .unwrap_or_else(|| "{}".to_string()),
                                        call_id: call_id.clone(),
                                        name: function_call.name,
                                        type_: ot::ResponseFunctionToolCallType::FunctionCall,
                                        id: Some(call_id),
                                        status: Some(ot::ResponseItemStatus::Completed),
                                    },
                                ));
                            }

                            if let Some(function_response) = part.function_response {
                                let call_id = function_response
                                    .id
                                    .clone()
                                    .unwrap_or_else(|| function_response.name.clone());
                                let output_text =
                                    gemini_function_response_to_text(function_response);
                                output.push(rt::ResponseOutputItem::FunctionCallOutput(
                                    ot::ResponseFunctionCallOutput {
                                        call_id,
                                        output: ot::ResponseFunctionCallOutputContent::Text(
                                            output_text,
                                        ),
                                        type_:
                                            ot::ResponseFunctionCallOutputType::FunctionCallOutput,
                                        id: None,
                                        status: Some(ot::ResponseItemStatus::Completed),
                                    },
                                ));
                            }

                            if let Some(executable_code) = part.executable_code {
                                tool_call_count += 1;
                                output.push(rt::ResponseOutputItem::CodeInterpreterToolCall(
                                    ot::ResponseCodeInterpreterToolCall {
                                        id: format!("code_interpreter_{candidate_index}_{part_index}"),
                                        code: executable_code.code,
                                        container_id: "gemini".to_string(),
                                        outputs: None,
                                        status: ot::ResponseCodeInterpreterToolCallStatus::Completed,
                                        type_: ot::ResponseCodeInterpreterToolCallType::CodeInterpreterCall,
                                    },
                                ));
                            }

                            if let Some(code_execution_result) = part.code_execution_result
                                && let Some(result_text) = code_execution_result.output
                                && !result_text.is_empty()
                            {
                                output.push(rt::ResponseOutputItem::FunctionCallOutput(
                                    ot::ResponseFunctionCallOutput {
                                        call_id: format!(
                                            "code_execution_{candidate_index}_{part_index}"
                                        ),
                                        output: ot::ResponseFunctionCallOutputContent::Text(
                                            result_text,
                                        ),
                                        type_:
                                            ot::ResponseFunctionCallOutputType::FunctionCallOutput,
                                        id: None,
                                        status: Some(ot::ResponseItemStatus::Completed),
                                    },
                                ));
                            }

                            if let Some(text) = part.text
                                && !text.is_empty()
                            {
                                output_text_parts.push(text.clone());
                                message_content.push(ot::ResponseOutputContent::Text(
                                    ot::ResponseOutputText {
                                        annotations: annotations.clone(),
                                        logprobs: if !logprobs_attached {
                                            logprobs_attached = true;
                                            logprobs.clone()
                                        } else {
                                            None
                                        },
                                        text,
                                        type_: ot::ResponseOutputTextType::OutputText,
                                    },
                                ));
                                continue;
                            }

                            if let Some(inline_data) = part.inline_data {
                                let text = format!(
                                    "data:{};base64,{}",
                                    inline_data.mime_type, inline_data.data
                                );
                                output_text_parts.push(text.clone());
                                message_content.push(ot::ResponseOutputContent::Text(
                                    ot::ResponseOutputText {
                                        annotations: Vec::new(),
                                        logprobs: None,
                                        text,
                                        type_: ot::ResponseOutputTextType::OutputText,
                                    },
                                ));
                            } else if let Some(file_data) = part.file_data {
                                output_text_parts.push(file_data.file_uri.clone());
                                message_content.push(ot::ResponseOutputContent::Text(
                                    ot::ResponseOutputText {
                                        annotations: Vec::new(),
                                        logprobs: None,
                                        text: file_data.file_uri,
                                        type_: ot::ResponseOutputTextType::OutputText,
                                    },
                                ));
                            }
                        }
                    }

                    if message_content.is_empty()
                        && let Some(finish_message) = candidate.finish_message
                        && !finish_message.is_empty()
                    {
                        output_text_parts.push(finish_message.clone());
                        message_content.push(ot::ResponseOutputContent::Text(
                            ot::ResponseOutputText {
                                annotations: Vec::new(),
                                logprobs: None,
                                text: finish_message,
                                type_: ot::ResponseOutputTextType::OutputText,
                            },
                        ));
                    }

                    if !message_content.is_empty() {
                        output.push(rt::ResponseOutputItem::Message(ot::ResponseOutputMessage {
                            id: format!("{}_message_{}", response_id, candidate_index),
                            content: message_content,
                            role: ot::ResponseOutputMessageRole::Assistant,
                            phase: Some(ot::ResponseMessagePhase::FinalAnswer),
                            status: Some(ot::ResponseItemStatus::Completed),
                            type_: Some(ot::ResponseOutputMessageType::Message),
                        }));
                    }
                }

                if output.is_empty()
                    && let Some(refusal) = prompt_feedback_refusal_text(prompt_feedback.as_ref())
                {
                    output.push(rt::ResponseOutputItem::Message(ot::ResponseOutputMessage {
                        id: format!("{}_message_0", response_id),
                        content: vec![ot::ResponseOutputContent::Refusal(
                            ot::ResponseOutputRefusal {
                                refusal,
                                type_: ot::ResponseOutputRefusalType::Refusal,
                            },
                        )],
                        role: ot::ResponseOutputMessageRole::Assistant,
                        phase: Some(ot::ResponseMessagePhase::FinalAnswer),
                        status: Some(ot::ResponseItemStatus::Completed),
                        type_: Some(ot::ResponseOutputMessageType::Message),
                    }));
                }

                let incomplete_reason = match first_finish_reason.as_ref() {
                    Some(gt::GeminiFinishReason::MaxTokens) => {
                        Some(rt::ResponseIncompleteReason::MaxOutputTokens)
                    }
                    Some(
                        gt::GeminiFinishReason::Safety
                        | gt::GeminiFinishReason::Recitation
                        | gt::GeminiFinishReason::Blocklist
                        | gt::GeminiFinishReason::ProhibitedContent
                        | gt::GeminiFinishReason::Spii
                        | gt::GeminiFinishReason::ImageSafety
                        | gt::GeminiFinishReason::ImageProhibitedContent
                        | gt::GeminiFinishReason::ImageRecitation,
                    ) => Some(rt::ResponseIncompleteReason::ContentFilter),
                    _ => None,
                }
                .or_else(|| {
                    match prompt_feedback
                        .as_ref()
                        .and_then(|feedback| feedback.block_reason.as_ref())
                    {
                        Some(gt::GeminiBlockReason::Safety)
                        | Some(gt::GeminiBlockReason::Blocklist)
                        | Some(gt::GeminiBlockReason::ProhibitedContent)
                        | Some(gt::GeminiBlockReason::ImageSafety) => {
                            Some(rt::ResponseIncompleteReason::ContentFilter)
                        }
                        _ => None,
                    }
                });
                let is_incomplete = incomplete_reason.is_some();

                OpenAiCreateResponseResponse::Success {
                    stats_code,
                    headers: OpenAiResponseHeaders {
                        extra: headers.extra,
                    },
                    body: ResponseBody {
                        id: response_id,
                        created_at: 0,
                        error: None,
                        incomplete_details: incomplete_reason.map(|reason| {
                            rt::ResponseIncompleteDetails {
                                reason: Some(reason),
                            }
                        }),
                        instructions: Some(ot::ResponseInput::Text(String::new())),
                        metadata: BTreeMap::new(),
                        model: response_model,
                        object: rt::ResponseObject::Response,
                        output,
                        parallel_tool_calls: tool_call_count > 1,
                        temperature: 1.0,
                        tool_choice: if tool_call_count > 0 {
                            ot::ResponseToolChoice::Options(ot::ResponseToolChoiceOptions::Required)
                        } else {
                            ot::ResponseToolChoice::Options(ot::ResponseToolChoiceOptions::Auto)
                        },
                        tools: Vec::new(),
                        top_p: 1.0,
                        background: None,
                        completed_at: None,
                        conversation: None,
                        max_output_tokens: None,
                        max_tool_calls: None,
                        output_text: if output_text_parts.is_empty() {
                            None
                        } else {
                            Some(output_text_parts.join("\n"))
                        },
                        previous_response_id: None,
                        prompt: None,
                        prompt_cache_key: None,
                        prompt_cache_retention: None,
                        reasoning: None,
                        safety_identifier: None,
                        service_tier: None,
                        status: Some(if is_incomplete {
                            rt::ResponseStatus::Incomplete
                        } else {
                            rt::ResponseStatus::Completed
                        }),
                        text: None,
                        top_logprobs: None,
                        truncation: None,
                        usage,
                        user: None,
                    },
                }
            }
            GeminiGenerateContentResponse::Error {
                stats_code,
                headers,
                body,
            } => OpenAiCreateResponseResponse::Error {
                stats_code,
                headers: OpenAiResponseHeaders {
                    extra: headers.extra,
                },
                body: openai_error_response_from_gemini(stats_code, body),
            },
        })
    }
}
