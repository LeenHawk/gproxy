//! Claude -> Gemini count-token transforms.

use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::common;

pub fn request(
    input: claude::CountTokensRequestBody,
    _: &TransformContext,
) -> Result<gemini::CountTokensRequest, TransformError> {
    let model = common::claude_model_string(&input.model);
    let contents = common::text_to_gemini_contents(common::claude_messages_to_text(input.messages));
    let system_instruction = common::claude_system_to_text(input.system).map(|text| {
        common::text_to_gemini_content(
            text,
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
        )
    });
    let tools = common::claude_tools_to_gemini(input.tools, input.mcp_servers);
    let tool_config = common::claude_tool_config_to_gemini(input.tool_choice);
    let generation_config = common::claude_generation_config_to_gemini(
        input.output_config,
        #[allow(deprecated)]
        input.output_format,
        input.thinking,
    );
    let service_tier = common::claude_service_tier_to_gemini(input.service_tier);

    let generate_content_request = gemini::GenerateContentRequest {
        model: Some(model.clone()),
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
    };

    Ok(gemini::CountTokensRequest {
        model: Some(model),
        contents: Vec::new(),
        generate_content_request: Some(generate_content_request),
        extra: Default::default(),
    })
}

pub fn response(
    input: claude::CountTokensResponseBody,
    _: &TransformContext,
) -> gemini::CountTokensResponse {
    gemini::CountTokensResponse {
        total_tokens: Some(common::u64_to_i32(input.input_tokens)),
        cached_content_token_count: input
            .context_management
            .and_then(|context| context.original_input_tokens)
            .map(common::u64_to_i32),
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        extra: Default::default(),
    }
}
