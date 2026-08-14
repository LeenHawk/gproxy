use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::content::gemini_content_to_response_output;
use super::usage::gemini_usage_to_response;

pub fn response(
    input: gemini::GenerateContentResponse,
    _: &TransformContext,
) -> Result<openai::ResponseObject, TransformError> {
    let usage = input.usage_metadata.map(gemini_usage_to_response);
    let mut output = Vec::new();
    let mut status = openai::ResponseStatus::Completed;
    let mut incomplete_details = None;
    for candidate in input.candidates {
        if matches!(
            candidate.finish_reason,
            Some(gemini::FinishReason::Known(
                gemini::FinishReasonKnown::MaxTokens
            ))
        ) {
            status = openai::ResponseStatus::Incomplete;
            incomplete_details = Some(crate::protocol::wire!(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                extra: Default::default(),
            }));
        }
        if let Some(content) = candidate.content {
            output.extend(gemini_content_to_response_output(content));
        }
    }
    let output_text = output
        .iter()
        .find_map(|item| match &item.0 {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => Some(
                message
                    .content
                    .iter()
                    .map(|part| match part {
                        openai::ResponseMessageOutputContentPart::OutputText { text, .. } => {
                            text.as_str()
                        }
                        openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
                            refusal.as_str()
                        }
                        _ => "",
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .filter(|text| !text.is_empty());
    Ok(crate::protocol::wire!(openai::ResponseObject {
        id: input.response_id.unwrap_or_default(),
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
        model: input.model_version.map(Into::into),
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
        service_tier: None,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn signed_visible_text_stays_output_text() {
        let input = serde_json::from_value(json!({
            "responseId": "response-id",
            "modelVersion": "gemini-3.1-flash-lite",
            "candidates": [{
                "finishReason": "STOP",
                "content": {
                    "role": "model",
                    "parts": [{"text": "ok", "thoughtSignature": "ciphertext"}]
                }
            }]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        );
        let output = serde_json::to_value(response(input, &ctx).unwrap()).unwrap();

        assert_eq!(output["output_text"], "ok");
        assert!(
            output["output"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| { item["type"] == "message" && item["content"][0]["text"] == "ok" })
        );
        let reasoning = output["output"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "reasoning")
            .unwrap();
        assert_eq!(reasoning["encrypted_content"], "ciphertext");
        assert!(reasoning.get("content").is_none());
    }
}
