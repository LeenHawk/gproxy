use crate::formats::{claude, gemini};
use std::collections::HashMap;
use uuid::Uuid;

use super::TransformError;

fn format_markdown_image(mime_type: &str, data: &str) -> String {
    format!("![gemini-generated-content](data:{mime_type};base64,{data})")
}

fn format_code_block(language: &str, code: &str) -> String {
    format!("\n```{language}\n{code}\n```\n")
}

fn format_code_output(label: &str, output: &str) -> String {
    format!("\n```{label}\n{output}\n```\n")
}

fn language_label(language: &gemini::types::Language) -> &'static str {
    match language {
        gemini::types::Language::Python => "python",
        _ => "text",
    }
}

fn outcome_label(outcome: &gemini::types::Outcome) -> &'static str {
    match outcome {
        gemini::types::Outcome::OutcomeOk => "output",
        _ => "error",
    }
}

fn map_stop_reason(
    finish_reason: Option<&gemini::types::FinishReason>,
    has_tool_use: bool,
) -> claude::messages::BetaStopReason {
    use gemini::types::FinishReason;
    use claude::messages::BetaStopReason;

    match finish_reason {
        Some(FinishReason::Stop) if has_tool_use => BetaStopReason::ToolUse,
        Some(FinishReason::MaxTokens) => BetaStopReason::MaxTokens,
        _ => BetaStopReason::EndTurn,
    }
}

fn usage_to_claude(
    usage: Option<&gemini::types::UsageMetadata>,
) -> claude::messages::BetaUsage {
    let prompt_tokens = usage
        .and_then(|usage| usage.prompt_token_count)
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|usage| usage.candidates_token_count)
        .unwrap_or(0);

    claude::messages::BetaUsage {
        cache_creation: None,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        input_tokens: prompt_tokens.max(0) as u32,
        output_tokens: output_tokens.max(0) as u32,
        server_tool_use: None,
        service_tier: claude::messages::UsageServiceTier::Standard,
    }
}

fn empty_text_block() -> claude::messages::BetaContentBlock {
    claude::messages::BetaContentBlock::Text(claude::messages::BetaTextBlock {
        citations: Vec::new(),
        text: String::new(),
        block_type: claude::types::TextBlockType::Text,
    })
}

fn text_block(text: String) -> claude::messages::BetaContentBlock {
    claude::messages::BetaContentBlock::Text(claude::messages::BetaTextBlock {
        citations: Vec::new(),
        text,
        block_type: claude::types::TextBlockType::Text,
    })
}

fn thinking_block(thinking: String, signature: Option<String>) -> claude::messages::BetaContentBlock {
    claude::messages::BetaContentBlock::Thinking(claude::messages::BetaThinkingBlock {
        signature,
        thinking,
        block_type: claude::types::ThinkingBlockType::Thinking,
    })
}

fn tool_use_block(
    id: String,
    name: String,
    input: HashMap<String, serde_json::Value>,
) -> claude::messages::BetaContentBlock {
    claude::messages::BetaContentBlock::ToolUse(claude::messages::BetaToolUseBlock {
        id,
        input,
        name,
        block_type: claude::types::ToolUseBlockType::ToolUse,
        caller: None,
    })
}

fn convert_parts(parts: &[gemini::types::Part]) -> (Vec<claude::messages::BetaContentBlock>, bool) {
    let mut content = Vec::new();
    let mut has_tool_use = false;

    for part in parts {
        if part.thought == Some(true) {
            let text = match &part.data {
                gemini::types::PartData::Text { text } => text.clone(),
                _ => String::new(),
            };
            content.push(thinking_block(text, part.thought_signature.clone()));
            continue;
        }

        match &part.data {
            gemini::types::PartData::Text { text } => {
                content.push(text_block(text.clone()));
            }
            gemini::types::PartData::InlineData { inline_data } => {
                content.push(text_block(format_markdown_image(
                    &inline_data.mime_type,
                    &inline_data.data,
                )));
            }
            gemini::types::PartData::ExecutableCode { executable_code } => {
                content.push(text_block(format_code_block(
                    language_label(&executable_code.language),
                    &executable_code.code,
                )));
            }
            gemini::types::PartData::CodeExecutionResult { code_execution_result } => {
                if let Some(output) = &code_execution_result.output {
                    content.push(text_block(format_code_output(
                        outcome_label(&code_execution_result.outcome),
                        output,
                    )));
                }
            }
            gemini::types::PartData::FunctionCall { function_call } => {
                has_tool_use = true;
                let id = function_call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("toolu_{}", Uuid::new_v4().simple()));
                let input = function_call
                    .args
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<HashMap<_, _>>();
                content.push(tool_use_block(id, function_call.name.clone(), input));
            }
            gemini::types::PartData::FunctionResponse { function_response } => {
                let serialized = serde_json::to_string(&function_response.response)
                    .unwrap_or_else(|_| "{}".to_string());
                content.push(text_block(serialized));
            }
            gemini::types::PartData::FileData { file_data } => {
                content.push(text_block(format!("file: {}", file_data.file_uri)));
            }
        }
    }

    if content.is_empty() {
        content.push(empty_text_block());
    }

    (content, has_tool_use)
}

pub fn response(
    body: gemini::generate_content::GenerateContentResponse,
    model: &str,
) -> Result<claude::messages::MessageCreateResponse, TransformError> {
    let candidate = body
        .candidates
        .first()
        .ok_or(TransformError::Missing("candidate"))?;

    let parts = candidate
        .content
        .as_ref()
        .map(|content| content.parts.as_slice())
        .unwrap_or(&[]);
    let (content, has_tool_use) = convert_parts(parts);

    let stop_reason = candidate
        .finish_reason
        .as_ref()
        .map(|reason| map_stop_reason(Some(reason), has_tool_use));

    let usage = usage_to_claude(body.usage_metadata.as_ref());

    Ok(claude::messages::MessageCreateResponse {
        id: format!("msg_{}", Uuid::new_v4().simple()),
        container: None,
        content,
        context_management: None,
        model: claude::types::Model::Other(model.to_string()),
        role: claude::messages::AssistantRole::Assistant,
        stop_reason,
        stop_sequence: None,
        message_type: claude::messages::MessageObjectType::Message,
        usage,
    })
}
