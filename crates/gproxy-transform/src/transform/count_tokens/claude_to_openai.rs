//! Claude -> OpenAI count-token transforms.

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::common;

pub fn request(
    mut input: claude::CountTokensRequestBody,
    _: &TransformContext,
) -> Result<openai::ResponseInputTokensRequest, TransformError> {
    crate::transform::common::apply_claude_message_controls(
        &mut input.messages,
        &mut input.output_config,
    );
    let tool_choice = input.tool_choice;
    let output_config = input.output_config;
    #[allow(deprecated)]
    let output_format = input.output_format;
    let previous_response_id = common::claude_previous_message_id_to_openai(input.diagnostics);

    Ok(crate::protocol::wire!(openai::ResponseInputTokensRequest {
        conversation: None,
        input: common::text_to_openai_input(common::claude_messages_to_text(input.messages)),
        instructions: common::claude_system_to_text(input.system),
        model: Some(common::claude_model_string(&input.model).into()),
        parallel_tool_calls: common::claude_parallel_tool_calls(tool_choice.as_ref()),
        personality: None,
        previous_response_id,
        prompt_cache_options: None,
        reasoning: common::claude_generation_to_openai_reasoning(
            input.thinking,
            output_config.as_ref(),
        ),
        service_tier: common::claude_speed_to_openai(input.speed)
            .or_else(|| common::claude_service_tier_to_openai(input.service_tier)),
        text: common::claude_generation_to_openai_text(output_config.as_ref(), output_format),
        tool_choice: common::claude_tool_choice_to_openai(tool_choice),
        tools: common::claude_tools_to_openai(input.tools, input.mcp_servers),
        truncation: None,
        extra: Default::default(),
    }))
}

pub fn response(
    input: claude::CountTokensResponseBody,
    _: &TransformContext,
) -> openai::ResponseInputTokensResponse {
    crate::protocol::wire!(openai::ResponseInputTokensResponse {
        input_tokens: common::u64_to_u32(input.input_tokens),
        object: openai::ResponseInputTokensObjectType::ResponseInputTokens,
        extra: Default::default(),
    })
}
