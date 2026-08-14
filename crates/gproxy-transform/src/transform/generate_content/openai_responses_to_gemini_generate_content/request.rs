use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::response_input_to_gemini_contents;
use super::tools::{response_tool_choice_to_gemini, response_tools_to_gemini};

pub fn request(
    input: openai::ResponseCreateRequest,
    _: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    let effort = input
        .reasoning
        .as_ref()
        .and_then(|reasoning| reasoning.effort.clone());
    let (response_mime_type, response_json_schema) = response_format(input.text);
    let tools = response_tools_to_gemini(input.tools);
    let tool_config = response_tool_choice_to_gemini(input.tool_choice);
    let system_instruction = input.instructions.map(|text| {
        crate::protocol::wire!(gemini::Content {
            parts: vec![crate::protocol::wire!(gemini::Part {
                data: Some(gemini::PartData::Text { text }),
                ..Default::default()
            })],
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
            extra: Default::default(),
        })
    });
    let generation_config = Some(crate::protocol::wire!(gemini::GenerationConfig {
        stop_sequences: Vec::new(),
        response_mime_type,
        response_schema: None,
        response_json_schema,
        response_modalities: Vec::new(),
        candidate_count: None,
        max_output_tokens: input.max_output_tokens.map(u32_to_i32),
        temperature: input.temperature,
        top_p: input.top_p,
        top_k: None,
        seed: None,
        presence_penalty: None,
        frequency_penalty: None,
        response_logprobs: None,
        logprobs: input.top_logprobs.map(u32_to_i32),
        enable_enhanced_civic_answers: None,
        speech_config: None,
        thinking_config: common::openai_reasoning_to_gemini(effort),
        media_resolution: None,
        image_config: None,
        extra: Default::default(),
    }));

    Ok(crate::protocol::wire!(gemini::GenerateContentRequest {
        model: input.model.map(common::openai_model_string),
        contents: response_input_to_gemini_contents(input.input),
        tools,
        tool_config,
        safety_settings: Vec::new(),
        system_instruction,
        generation_config,
        cached_content: input.prompt_cache_key,
        service_tier: common::openai_service_tier_to_gemini(input.service_tier),
        store: input.store,
        extra: Default::default(),
    }))
}

fn response_format(
    text: Option<openai::TextConfig>,
) -> (Option<gemini::ResponseMimeType>, Option<serde_json::Value>) {
    match text.and_then(|text| text.format) {
        Some(openai::ResponseFormat::Text(_)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::TextPlain,
            )),
            None,
        ),
        Some(openai::ResponseFormat::JsonObject(_)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            )),
            None,
        ),
        Some(openai::ResponseFormat::JsonSchema(format)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            )),
            Some(serde_json::Value::Object(
                format.schema.into_iter().collect(),
            )),
        ),
        None => (None, None),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn maps_encrypted_reasoning_directly_to_thought_signature() {
        let input = serde_json::from_value(json!({
            "model": "gemini-3-pro",
            "input": [{
                "type": "reasoning",
                "summary": [],
                "content": [{"type": "reasoning_text", "text": "hidden"}],
                "encrypted_content": "ciphertext"
            }]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
        );
        let output = serde_json::to_value(request(input, &ctx).unwrap()).unwrap();
        assert_eq!(
            output["contents"][0]["parts"][0]["thoughtSignature"],
            "ciphertext"
        );
        assert_eq!(output["contents"][0]["parts"][0]["text"], "hidden");
    }
}
