use crate::gemini::count_tokens::types::{GeminiContentRole, GeminiFunctionCall, GeminiPart};
use crate::gemini::generate_content::response::{GeminiGenerateContentResponse, ResponseBody};
use crate::gemini::generate_content::types::{
    GeminiBlockReason, GeminiCandidate, GeminiContent, GeminiFinishReason, GeminiPromptFeedback,
    GeminiUsageMetadata,
};
use crate::gemini::types::GeminiResponseHeaders;
use crate::openai::count_tokens::types::{
    ResponseCustomToolCallOutputContent, ResponseFunctionCallOutputContent, ResponseInputContent,
};
use crate::openai::create_response::response::OpenAiCreateResponseResponse;
use crate::openai::create_response::types::{ResponseIncompleteReason, ResponseOutputItem};
use crate::transform::gemini::generate_content::utils::{
    gemini_error_response_from_openai, parse_json_object_or_empty,
};
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiCreateResponseResponse> for GeminiGenerateContentResponse {
    type Error = TransformError;

    fn try_from(value: OpenAiCreateResponseResponse) -> Result<Self, TransformError> {
        Ok(match value {
            OpenAiCreateResponseResponse::Success {
                stats_code,
                headers,
                body,
            } => {
                let input_content_to_text = |items: Vec<ResponseInputContent>| {
                    items
                        .into_iter()
                        .filter_map(|item| match item {
                            ResponseInputContent::Text(text) => Some(text.text),
                            ResponseInputContent::Image(image) => {
                                if let Some(url) = image.image_url {
                                    Some(url)
                                } else {
                                    image.file_id.map(|file_id| format!("file:{file_id}"))
                                }
                            }
                            ResponseInputContent::File(file) => {
                                if let Some(data) = file.file_data {
                                    Some(data)
                                } else if let Some(url) = file.file_url {
                                    Some(url)
                                } else if let Some(file_id) = file.file_id {
                                    Some(format!("file:{file_id}"))
                                } else {
                                    file.filename
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                let mut parts = Vec::new();
                let mut has_tool_call = false;
                let mut has_refusal = false;

                for item in body.output {
                    match item {
                        ResponseOutputItem::Message(message) => {
                            for content in message.content {
                                match content {
                                    crate::openai::count_tokens::types::ResponseOutputContent::Text(
                                        text,
                                    ) => {
                                        if !text.text.is_empty() {
                                            parts.push(GeminiPart {
                                                text: Some(text.text),
                                                ..GeminiPart::default()
                                            });
                                        }
                                    }
                                    crate::openai::count_tokens::types::ResponseOutputContent::Refusal(
                                        refusal,
                                    ) => {
                                        has_refusal = true;
                                        if !refusal.refusal.is_empty() {
                                            parts.push(GeminiPart {
                                                text: Some(refusal.refusal),
                                                ..GeminiPart::default()
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        ResponseOutputItem::FunctionToolCall(call) => {
                            has_tool_call = true;
                            parts.push(GeminiPart {
                                function_call: Some(GeminiFunctionCall {
                                    id: call.id.or(Some(call.call_id)),
                                    name: call.name,
                                    args: Some(parse_json_object_or_empty(&call.arguments)),
                                }),
                                ..GeminiPart::default()
                            });
                        }
                        ResponseOutputItem::CustomToolCall(call) => {
                            has_tool_call = true;
                            parts.push(GeminiPart {
                                function_call: Some(GeminiFunctionCall {
                                    id: call.id.or(Some(call.call_id)),
                                    name: call.name,
                                    args: Some(parse_json_object_or_empty(&call.input)),
                                }),
                                ..GeminiPart::default()
                            });
                        }
                        ResponseOutputItem::ReasoningItem(item) => {
                            let thought_signature = item.id.clone().filter(|id| !id.is_empty());
                            for summary in item.summary {
                                if !summary.text.is_empty() {
                                    parts.push(GeminiPart {
                                        thought: Some(true),
                                        thought_signature: thought_signature.clone(),
                                        text: Some(summary.text),
                                        ..GeminiPart::default()
                                    });
                                }
                            }
                            if let Some(content) = item.content {
                                for reasoning_text in content {
                                    if !reasoning_text.text.is_empty() {
                                        parts.push(GeminiPart {
                                            thought: Some(true),
                                            thought_signature: thought_signature.clone(),
                                            text: Some(reasoning_text.text),
                                            ..GeminiPart::default()
                                        });
                                    }
                                }
                            }
                            if let Some(encrypted_content) = item.encrypted_content
                                && !encrypted_content.is_empty()
                            {
                                parts.push(GeminiPart {
                                    thought: Some(true),
                                    thought_signature,
                                    text: Some(encrypted_content),
                                    ..GeminiPart::default()
                                });
                            }
                        }
                        ResponseOutputItem::FunctionCallOutput(call) => {
                            let output = match call.output {
                                ResponseFunctionCallOutputContent::Text(text) => text,
                                ResponseFunctionCallOutputContent::Content(items) => {
                                    input_content_to_text(items)
                                }
                            };
                            if !output.is_empty() {
                                parts.push(GeminiPart {
                                    text: Some(output),
                                    ..GeminiPart::default()
                                });
                            }
                        }
                        ResponseOutputItem::CustomToolCallOutput(call) => {
                            let output = match call.output {
                                ResponseCustomToolCallOutputContent::Text(text) => text,
                                ResponseCustomToolCallOutputContent::Content(items) => {
                                    input_content_to_text(items)
                                }
                            };
                            if !output.is_empty() {
                                parts.push(GeminiPart {
                                    text: Some(output),
                                    ..GeminiPart::default()
                                });
                            }
                        }
                        ResponseOutputItem::ShellCallOutput(call) => {
                            let output = call
                                .output
                                .into_iter()
                                .map(|entry| {
                                    format!("stdout: {}\nstderr: {}", entry.stdout, entry.stderr)
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            if !output.is_empty() {
                                parts.push(GeminiPart {
                                    text: Some(output),
                                    ..GeminiPart::default()
                                });
                            }
                        }
                        ResponseOutputItem::LocalShellCallOutput(call)
                            if !call.output.is_empty() =>
                        {
                            parts.push(GeminiPart {
                                text: Some(call.output),
                                ..GeminiPart::default()
                            });
                        }
                        ResponseOutputItem::McpCall(call) => {
                            if let Some(output) = call.output
                                && !output.is_empty()
                            {
                                parts.push(GeminiPart {
                                    text: Some(output),
                                    ..GeminiPart::default()
                                });
                            }
                            if let Some(error) = call.error
                                && !error.is_empty()
                            {
                                has_refusal = true;
                                parts.push(GeminiPart {
                                    text: Some(error),
                                    ..GeminiPart::default()
                                });
                            }
                        }
                        ResponseOutputItem::ImageGenerationCall(call)
                            if call.result.as_deref().is_some_and(|s| !s.is_empty()) =>
                        {
                            parts.push(GeminiPart {
                                text: Some(call.result.unwrap_or_default()),
                                ..GeminiPart::default()
                            });
                        }
                        _ => {}
                    }
                }

                if parts.is_empty() {
                    parts.push(GeminiPart {
                        text: body.output_text.clone().or(Some(String::new())),
                        ..GeminiPart::default()
                    });
                }

                let incomplete_reason = body
                    .incomplete_details
                    .as_ref()
                    .and_then(|details| details.reason.as_ref());
                let finish_reason = match incomplete_reason {
                    Some(ResponseIncompleteReason::MaxOutputTokens) => {
                        Some(GeminiFinishReason::MaxTokens)
                    }
                    Some(ResponseIncompleteReason::ContentFilter) => {
                        Some(GeminiFinishReason::Safety)
                    }
                    None => {
                        if has_tool_call {
                            Some(GeminiFinishReason::UnexpectedToolCall)
                        } else if has_refusal {
                            Some(GeminiFinishReason::Safety)
                        } else {
                            Some(GeminiFinishReason::Stop)
                        }
                    }
                };

                let prompt_feedback = if matches!(
                    incomplete_reason,
                    Some(ResponseIncompleteReason::ContentFilter)
                ) {
                    Some(GeminiPromptFeedback {
                        block_reason: Some(GeminiBlockReason::Safety),
                        safety_ratings: None,
                    })
                } else {
                    None
                };

                let usage_metadata = body.usage.map(|usage| GeminiUsageMetadata {
                    prompt_token_count: Some(usage.input_tokens),
                    cached_content_token_count: Some(usage.input_tokens_details.cached_tokens),
                    candidates_token_count: Some(usage.output_tokens),
                    thoughts_token_count: Some(usage.output_tokens_details.reasoning_tokens),
                    total_token_count: Some(usage.total_tokens),
                    ..GeminiUsageMetadata::default()
                });

                GeminiGenerateContentResponse::Success {
                    stats_code,
                    headers: GeminiResponseHeaders {
                        extra: headers.extra,
                    },
                    body: ResponseBody {
                        candidates: Some(vec![GeminiCandidate {
                            content: Some(GeminiContent {
                                parts,
                                role: Some(GeminiContentRole::Model),
                            }),
                            finish_reason,
                            index: Some(0),
                            ..GeminiCandidate::default()
                        }]),
                        prompt_feedback,
                        usage_metadata,
                        model_version: Some(body.model),
                        response_id: Some(body.id),
                        model_status: None,
                    },
                }
            }
            OpenAiCreateResponseResponse::Error {
                stats_code,
                headers,
                body,
            } => GeminiGenerateContentResponse::Error {
                stats_code,
                headers: GeminiResponseHeaders {
                    extra: headers.extra,
                },
                body: gemini_error_response_from_openai(stats_code, body),
            },
        })
    }
}
