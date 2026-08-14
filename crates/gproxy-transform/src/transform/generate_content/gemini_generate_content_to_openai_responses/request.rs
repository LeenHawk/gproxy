use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::super::gemini_generate_content_to_openai_chat::tools::{
    gemini_tool_config_to_chat, gemini_tools_to_chat,
};
use super::super::openai_chat_to_openai_responses::tools::{
    chat_tool_choice_to_response_tool_choice, chat_tools_to_response_tools,
};
use super::content::gemini_contents_to_response_items;

pub fn request(
    input: gemini::GenerateContentRequest,
    _: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    let generation = input.generation_config;
    let effort = common::gemini_thinking_to_openai(
        generation
            .as_ref()
            .and_then(|config| config.thinking_config.as_ref()),
    );
    let text = gemini_text_config(generation.as_ref());
    let tools = chat_tools_to_response_tools(Some(gemini_tools_to_chat(input.tools)));
    let tool_choice =
        chat_tool_choice_to_response_tool_choice(gemini_tool_config_to_chat(input.tool_config));

    Ok(crate::protocol::wire!(openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(
            gemini_contents_to_response_items(input.contents)
        )),
        instructions: input
            .system_instruction
            .map(gemini_content_text)
            .filter(|text| !text.is_empty()),
        max_output_tokens: generation
            .as_ref()
            .and_then(|config| config.max_output_tokens)
            .map(i32_to_u32),
        max_tool_calls: None,
        metadata: None,
        model: input.model.map(Into::into),
        moderation: None,
        multi_agent: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        prompt_cache_key: input.cached_content,
        prompt_cache_options: None,
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
        service_tier: common::gemini_service_tier_to_openai(input.service_tier),
        store: input.store,
        stream: None,
        stream_options: None,
        temperature: generation.as_ref().and_then(|config| config.temperature),
        text,
        tool_choice,
        tools,
        top_logprobs: generation
            .as_ref()
            .and_then(|config| config.logprobs)
            .map(i32_to_u32),
        top_p: generation.as_ref().and_then(|config| config.top_p),
        truncation: None,
        user: None,
        extra: Default::default(),
    }))
}

fn gemini_content_text(content: gemini::Content) -> String {
    content
        .parts
        .into_iter()
        .filter_map(|part| match part.data {
            Some(gemini::PartData::Text { text }) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn gemini_text_config(config: Option<&gemini::GenerationConfig>) -> Option<openai::TextConfig> {
    let mime = config?.response_mime_type.as_ref()?;
    let format = match mime {
        gemini::ResponseMimeType::Known(gemini::ResponseMimeTypeKnown::TextPlain) => {
            openai::ResponseFormat::Text(crate::protocol::wire!(openai::TextResponseFormat {
                type_: openai::TextResponseFormatType::Text,
                extra: Default::default(),
            }))
        }
        _ => openai::ResponseFormat::JsonObject(crate::protocol::wire!(
            openai::JsonObjectResponseFormat {
                type_: openai::JsonObjectResponseFormatType::JsonObject,
                extra: Default::default(),
            }
        )),
    };
    Some(crate::protocol::wire!(openai::TextConfig {
        format: Some(format),
        verbosity: None,
        extra: Default::default(),
    }))
}

fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn maps_thought_signature_directly_to_encrypted_reasoning() {
        let input = serde_json::from_value(json!({
            "model": "gemini-3-pro",
            "contents": [{
                "role": "model",
                "parts": [{"text": "hidden", "thought": true, "thoughtSignature": "ciphertext"}]
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
        let output = serde_json::to_value(request(input, &ctx).unwrap()).unwrap();
        let reasoning = output["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "reasoning")
            .unwrap();
        assert_eq!(reasoning["encrypted_content"], "ciphertext");
        assert_eq!(reasoning["content"][0]["text"], "hidden");
    }
}
