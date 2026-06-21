//! Gemini -> Claude count-token transforms.

use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::common;

pub fn request(
    input: gemini::CountTokensRequest,
    _: &TransformContext,
) -> Result<claude::CountTokensRequestBody, TransformError> {
    let request = common::split_gemini_count_token_request(input);
    let tool_parts = common::gemini_tools_to_claude(request.tools);

    #[allow(deprecated)]
    Ok(claude::CountTokensRequestBody {
        model: common::gemini_model_string(request.model).into(),
        messages: common::text_to_claude_messages(common::gemini_contents_to_text(
            request.contents,
        )),
        cache_control: None,
        context_management: None,
        diagnostics: None,
        mcp_servers: tool_parts.mcp_servers,
        output_config: common::gemini_generation_to_claude_output_config(
            request.generation_config.as_ref(),
        ),
        output_format: common::gemini_generation_to_claude_output_format(
            request.generation_config.as_ref(),
        ),
        service_tier: common::gemini_service_tier_to_claude(request.service_tier),
        speed: None,
        system: common::text_to_claude_system(
            request.system_instruction.map(common::gemini_content_text),
        ),
        thinking: common::gemini_generation_to_claude_thinking(request.generation_config.as_ref()),
        tool_choice: common::gemini_tool_config_to_claude(request.tool_config),
        tools: tool_parts.tools,
        extra: Default::default(),
    })
}

pub fn response(
    input: gemini::CountTokensResponse,
    _: &TransformContext,
) -> claude::CountTokensResponseBody {
    claude::CountTokensResponseBody {
        input_tokens: input
            .total_tokens
            .map(common::i32_to_u64)
            .unwrap_or_default(),
        context_management: input.cached_content_token_count.map(|cached| {
            claude::CountTokensContextManagement {
                original_input_tokens: Some(common::i32_to_u64(cached)),
                extra: Default::default(),
            }
        }),
        extra: Default::default(),
    }
}
