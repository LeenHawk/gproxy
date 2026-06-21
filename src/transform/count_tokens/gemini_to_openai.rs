//! Gemini -> OpenAI count-token transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::common;

pub fn request(
    input: gemini::CountTokensRequest,
    _: &TransformContext,
) -> Result<openai::ResponseInputTokensRequest, TransformError> {
    let request = common::split_gemini_count_token_request(input);

    Ok(openai::ResponseInputTokensRequest {
        conversation: None,
        input: common::text_to_openai_input(common::gemini_contents_to_text(request.contents)),
        instructions: request.system_instruction.map(common::gemini_content_text),
        model: Some(common::gemini_model_string(request.model).into()),
        parallel_tool_calls: None,
        personality: None,
        previous_response_id: None,
        reasoning: common::gemini_generation_to_openai_reasoning(
            request.generation_config.as_ref(),
        ),
        service_tier: common::gemini_service_tier_to_openai(request.service_tier),
        text: common::gemini_generation_to_openai_text(request.generation_config.as_ref()),
        tool_choice: common::gemini_tool_config_to_openai(request.tool_config),
        tools: common::gemini_tools_to_openai(request.tools),
        truncation: None,
        extra: Default::default(),
    })
}

pub fn response(
    input: gemini::CountTokensResponse,
    _: &TransformContext,
) -> openai::ResponseInputTokensResponse {
    openai::ResponseInputTokensResponse {
        input_tokens: input
            .total_tokens
            .map(common::i32_to_u32)
            .unwrap_or_default(),
        object: openai::ResponseInputTokensObjectType::ResponseInputTokens,
        extra: Default::default(),
    }
}
