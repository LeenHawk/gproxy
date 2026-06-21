//! OpenAI -> Gemini count-token transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::common;

pub fn request(
    input: openai::ResponseInputTokensRequest,
    _: &TransformContext,
) -> Result<gemini::CountTokensRequest, TransformError> {
    let model = common::openai_model_string(input.model);
    let contents = common::text_to_gemini_contents(common::openai_input_to_text(input.input));
    let system_instruction = input
        .instructions
        .filter(|text| !text.is_empty())
        .map(|text| {
            common::text_to_gemini_content(
                text,
                Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
            )
        });
    let tools = common::openai_tools_to_gemini(input.tools);
    let tool_config = common::openai_tool_config_to_gemini(input.tool_choice);
    let generation_config = common::openai_generation_config_to_gemini(input.reasoning, input.text);
    let service_tier = common::openai_service_tier_to_gemini(input.service_tier);

    Ok(gemini::CountTokensRequest {
        model: Some(model.clone()),
        contents: Vec::new(),
        generate_content_request: Some(gemini::GenerateContentRequest {
            model: Some(model),
            contents,
            tools,
            tool_config,
            safety_settings: Vec::new(),
            system_instruction,
            generation_config,
            cached_content: None,
            service_tier,
            store: None,
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}

pub fn response(
    input: openai::ResponseInputTokensResponse,
    _: &TransformContext,
) -> gemini::CountTokensResponse {
    gemini::CountTokensResponse {
        total_tokens: Some(common::u32_to_i32(input.input_tokens)),
        cached_content_token_count: None,
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        extra: Default::default(),
    }
}
