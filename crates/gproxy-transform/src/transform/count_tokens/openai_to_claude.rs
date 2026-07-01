//! OpenAI -> Claude count-token transforms.

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::common;

pub fn request(
    input: openai::ResponseInputTokensRequest,
    _: &TransformContext,
) -> Result<claude::CountTokensRequestBody, TransformError> {
    let output_config = common::openai_generation_to_claude_output_config(
        input.reasoning.as_ref(),
        input.text.as_ref(),
    );
    let output_format = common::openai_text_to_claude_output_format(input.text);
    let mcp_servers = common::openai_mcp_servers_to_claude(input.tools.as_deref());
    let tool_choice =
        common::openai_tool_choice_to_claude(input.tool_choice, input.parallel_tool_calls);

    #[allow(deprecated)]
    Ok(claude::CountTokensRequestBody {
        model: common::openai_model_string(input.model).into(),
        messages: common::text_to_claude_messages(common::openai_input_to_text(input.input)),
        cache_control: None,
        context_management: None,
        diagnostics: common::openai_previous_response_id_to_claude(input.previous_response_id),
        mcp_servers,
        output_config,
        output_format,
        service_tier: common::openai_service_tier_to_claude(input.service_tier),
        speed: None,
        system: common::text_to_claude_system(input.instructions),
        thinking: common::openai_reasoning_to_claude(input.reasoning),
        tool_choice,
        tools: common::openai_tools_to_claude(input.tools),
        extra: Default::default(),
    })
}

pub fn response(
    input: openai::ResponseInputTokensResponse,
    _: &TransformContext,
) -> claude::CountTokensResponseBody {
    claude::CountTokensResponseBody {
        input_tokens: common::u32_to_u64(input.input_tokens),
        context_management: None,
        extra: Default::default(),
    }
}
