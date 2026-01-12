use crate::formats::{gemini, openai};
use time::OffsetDateTime;
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

fn push_extra_content(content: &mut String, extra: &str) {
    if extra.is_empty() {
        return;
    }
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(extra);
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

fn map_finish_reason(
    finish_reason: Option<&gemini::types::FinishReason>,
    has_tool_calls: bool,
) -> openai::chat_completions::ChatCompletionFinishReason {
    use gemini::types::FinishReason;
    use openai::chat_completions::ChatCompletionFinishReason;

    match finish_reason {
        Some(FinishReason::Stop) if has_tool_calls => ChatCompletionFinishReason::ToolCalls,
        Some(FinishReason::Stop) => ChatCompletionFinishReason::Stop,
        Some(FinishReason::MaxTokens) => ChatCompletionFinishReason::Length,
        Some(
            FinishReason::Safety
            | FinishReason::Recitation
            | FinishReason::Blocklist
            | FinishReason::ProhibitedContent
            | FinishReason::Spii
            | FinishReason::ImageSafety
            | FinishReason::ImageProhibitedContent
            | FinishReason::ImageRecitation,
        ) => ChatCompletionFinishReason::ContentFilter,
        _ => ChatCompletionFinishReason::Stop,
    }
}

fn usage_to_openai(
    usage: Option<&gemini::types::UsageMetadata>,
) -> Option<openai::chat_completions::CompletionUsage> {
    let usage = usage?;
    let prompt_tokens = usage.prompt_token_count.unwrap_or(0);
    let completion_tokens = usage
        .candidates_token_count
        .or_else(|| usage.total_token_count.map(|total| total.saturating_sub(prompt_tokens)))
        .unwrap_or(0);
    let total_tokens = usage
        .total_token_count
        .unwrap_or(prompt_tokens.saturating_add(completion_tokens));

    Some(openai::chat_completions::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens,
        completion_tokens_details: None,
        prompt_tokens_details: None,
        extra_properties: None,
    })
}

fn convert_parts(
    parts: &[gemini::types::Part],
) -> (Option<String>, Vec<openai::chat_completions::ChatCompletionMessageToolCall>) {
    let mut content = String::new();
    let mut extras: Vec<String> = Vec::new();
    let mut tool_calls: Vec<openai::chat_completions::ChatCompletionMessageToolCall> = Vec::new();

    for part in parts {
        if part.thought == Some(true) {
            continue;
        }

        match &part.data {
            gemini::types::PartData::Text { text } => {
                content.push_str(text);
            }
            gemini::types::PartData::InlineData { inline_data } => {
                extras.push(format_markdown_image(&inline_data.mime_type, &inline_data.data));
            }
            gemini::types::PartData::ExecutableCode { executable_code } => {
                extras.push(format_code_block(
                    language_label(&executable_code.language),
                    &executable_code.code,
                ));
            }
            gemini::types::PartData::CodeExecutionResult { code_execution_result } => {
                if let Some(output) = &code_execution_result.output {
                    extras.push(format_code_output(
                        outcome_label(&code_execution_result.outcome),
                        output,
                    ));
                }
            }
            gemini::types::PartData::FunctionCall { function_call } => {
                let id = function_call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
                let args_json = function_call
                    .args
                    .as_ref()
                    .and_then(|args| serde_json::to_string(args).ok())
                    .unwrap_or_else(|| "{}".to_string());
                tool_calls.push(openai::chat_completions::ChatCompletionMessageToolCall::Function {
                    id,
                    function: openai::chat_completions::ChatCompletionToolCallFunction {
                        name: function_call.name.clone(),
                        arguments: args_json,
                    },
                });
            }
            gemini::types::PartData::FunctionResponse { function_response } => {
                let serialized = serde_json::to_string(&function_response.response)
                    .unwrap_or_else(|_| "{}".to_string());
                extras.push(serialized);
            }
            gemini::types::PartData::FileData { file_data } => {
                extras.push(format!("file: {}", file_data.file_uri));
            }
        }
    }

    if !extras.is_empty() {
        let extra = extras.join("\n\n");
        push_extra_content(&mut content, &extra);
    }

    let content = if content.is_empty() { None } else { Some(content) };
    (content, tool_calls)
}

pub fn response(
    body: gemini::generate_content::GenerateContentResponse,
    model: &str,
) -> Result<openai::chat_completions::CreateChatCompletionResponse, TransformError> {
    let created = OffsetDateTime::now_utc().unix_timestamp();
    let response_id = format!("chatcmpl-{}", Uuid::new_v4().simple());

    let mut choices = Vec::new();
    for (idx, candidate) in body.candidates.iter().enumerate() {
        let parts = candidate
            .content
            .as_ref()
            .map(|content| content.parts.as_slice())
            .unwrap_or(&[]);
        let (content, tool_calls) = convert_parts(parts);
        let has_tool_calls = !tool_calls.is_empty();
        let finish_reason = map_finish_reason(candidate.finish_reason.as_ref(), has_tool_calls);

        let message = openai::chat_completions::ChatCompletionResponseMessage {
            content,
            refusal: None,
            tool_calls: if has_tool_calls { Some(tool_calls) } else { None },
            annotations: None,
            role: openai::chat_completions::ChatCompletionResponseRole::Assistant,
            function_call: None,
            audio: None,
        };

        choices.push(openai::chat_completions::ChatCompletionChoice {
            finish_reason,
            index: candidate.index.unwrap_or(idx as i64),
            message,
            logprobs: None,
        });
    }

    let usage = usage_to_openai(body.usage_metadata.as_ref());

    Ok(openai::chat_completions::CreateChatCompletionResponse {
        id: response_id,
        choices,
        created,
        model: model.to_string(),
        service_tier: None,
        system_fingerprint: None,
        object_type: openai::chat_completions::ChatCompletionObjectType::ChatCompletion,
        usage,
    })
}
