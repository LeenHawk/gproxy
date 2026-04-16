use crate::claude::count_tokens::types::BetaServerToolUseName;
use crate::claude::create_message::response::ClaudeCreateMessageResponse;
use crate::claude::create_message::types::{BetaContentBlock, BetaStopReason};
use crate::gemini::count_tokens::types::{GeminiContentRole, GeminiFunctionCall, GeminiPart};
use crate::gemini::generate_content::response::{GeminiGenerateContentResponse, ResponseBody};
use crate::gemini::generate_content::types::{
    GeminiCandidate, GeminiContent, GeminiFinishReason, GeminiUsageMetadata,
};
use crate::gemini::types::GeminiResponseHeaders;
use crate::transform::claude::utils::claude_model_to_string;
use crate::transform::gemini::generate_content::utils::gemini_error_response_from_claude;
use crate::transform::utils::TransformError;

impl TryFrom<ClaudeCreateMessageResponse> for GeminiGenerateContentResponse {
    type Error = TransformError;

    fn try_from(value: ClaudeCreateMessageResponse) -> Result<Self, TransformError> {
        Ok(match value {
            ClaudeCreateMessageResponse::Success {
                stats_code,
                headers,
                body,
            } => {
                let mut parts = Vec::new();
                for block in body.content {
                    match block {
                        BetaContentBlock::Text(block) if !block.text.is_empty() => {
                            parts.push(GeminiPart {
                                text: Some(block.text),
                                ..GeminiPart::default()
                            });
                        }
                        BetaContentBlock::Thinking(block) if !block.thinking.is_empty() => {
                            parts.push(GeminiPart {
                                thought: Some(true),
                                thought_signature: Some(block.signature),
                                text: Some(block.thinking),
                                ..GeminiPart::default()
                            });
                        }
                        BetaContentBlock::ToolUse(block) => {
                            parts.push(GeminiPart {
                                function_call: Some(GeminiFunctionCall {
                                    id: Some(block.id),
                                    name: block.name,
                                    args: Some(block.input),
                                }),
                                ..GeminiPart::default()
                            });
                        }
                        BetaContentBlock::ServerToolUse(block) => {
                            let name = match block.name {
                                BetaServerToolUseName::WebSearch => "web_search",
                                BetaServerToolUseName::WebFetch => "web_fetch",
                                BetaServerToolUseName::CodeExecution => "code_execution",
                                BetaServerToolUseName::BashCodeExecution => "bash_code_execution",
                                BetaServerToolUseName::TextEditorCodeExecution => {
                                    "text_editor_code_execution"
                                }
                                BetaServerToolUseName::ToolSearchToolRegex => "tool_search_regex",
                                BetaServerToolUseName::ToolSearchToolBm25 => "tool_search_bm25",
                            }
                            .to_string();
                            parts.push(GeminiPart {
                                function_call: Some(GeminiFunctionCall {
                                    id: Some(block.id),
                                    name,
                                    args: Some(block.input),
                                }),
                                ..GeminiPart::default()
                            });
                        }
                        BetaContentBlock::McpToolUse(block) => {
                            parts.push(GeminiPart {
                                function_call: Some(GeminiFunctionCall {
                                    id: Some(block.id),
                                    name: format!("mcp:{}:{}", block.server_name, block.name),
                                    args: Some(block.input),
                                }),
                                ..GeminiPart::default()
                            });
                        }
                        _ => {}
                    }
                }

                if parts.is_empty() {
                    parts.push(GeminiPart {
                        text: Some(String::new()),
                        ..GeminiPart::default()
                    });
                }

                let finish_reason = match body.stop_reason {
                    Some(BetaStopReason::MaxTokens)
                    | Some(BetaStopReason::ModelContextWindowExceeded) => {
                        Some(GeminiFinishReason::MaxTokens)
                    }
                    Some(BetaStopReason::ToolUse) => Some(GeminiFinishReason::UnexpectedToolCall),
                    Some(BetaStopReason::Refusal) => Some(GeminiFinishReason::Safety),
                    Some(BetaStopReason::Compaction) => Some(GeminiFinishReason::Other),
                    Some(BetaStopReason::PauseTurn) => Some(GeminiFinishReason::Other),
                    Some(BetaStopReason::EndTurn) | Some(BetaStopReason::StopSequence) | None => {
                        Some(GeminiFinishReason::Stop)
                    }
                };

                let usage_metadata = GeminiUsageMetadata {
                    prompt_token_count: Some(
                        body.usage
                            .input_tokens
                            .saturating_add(body.usage.cache_creation_input_tokens),
                    ),
                    cached_content_token_count: Some(body.usage.cache_read_input_tokens),
                    candidates_token_count: Some(body.usage.output_tokens),
                    total_token_count: Some(
                        body.usage
                            .input_tokens
                            .saturating_add(body.usage.cache_creation_input_tokens)
                            .saturating_add(body.usage.cache_read_input_tokens)
                            .saturating_add(body.usage.output_tokens),
                    ),
                    ..GeminiUsageMetadata::default()
                };

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
                            token_count: Some(body.usage.output_tokens),
                            index: Some(0),
                            ..GeminiCandidate::default()
                        }]),
                        prompt_feedback: None,
                        usage_metadata: Some(usage_metadata),
                        model_version: Some(claude_model_to_string(&body.model)),
                        response_id: Some(body.id),
                        model_status: None,
                    },
                }
            }
            ClaudeCreateMessageResponse::Error {
                stats_code,
                headers,
                body,
            } => GeminiGenerateContentResponse::Error {
                stats_code,
                headers: GeminiResponseHeaders {
                    extra: headers.extra,
                },
                body: gemini_error_response_from_claude(stats_code, body),
            },
        })
    }
}
