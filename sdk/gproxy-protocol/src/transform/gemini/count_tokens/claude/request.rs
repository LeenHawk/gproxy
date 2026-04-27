use crate::claude::count_tokens::request::{
    ClaudeCountTokensRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::claude::count_tokens::types::{HttpMethod, Model};
use crate::gemini::count_tokens::request::GeminiCountTokensRequest;
use crate::transform::gemini::utils::{
    claude_output_config_from_effort_and_format,
    claude_thinking_effort_format_from_gemini_generation_config,
    gemini_contents_to_claude_messages, gemini_system_instruction_to_claude,
    gemini_tool_choice_to_claude, gemini_tools_to_claude, strip_models_prefix,
};
use crate::transform::utils::TransformError;

impl TryFrom<GeminiCountTokensRequest> for ClaudeCountTokensRequest {
    type Error = TransformError;

    fn try_from(value: GeminiCountTokensRequest) -> Result<Self, TransformError> {
        let (model_name, contents, tools, tool_config, system_instruction, generation_config) =
            if let Some(generate_content_request) = value.body.generate_content_request {
                (
                    generate_content_request.model,
                    generate_content_request.contents,
                    generate_content_request.tools,
                    generate_content_request.tool_config,
                    generate_content_request.system_instruction,
                    generate_content_request.generation_config,
                )
            } else {
                (
                    value.path.model,
                    value.body.contents.unwrap_or_default(),
                    None,
                    None,
                    None,
                    None,
                )
            };

        let model = Model::Custom(strip_models_prefix(&model_name));
        let messages = gemini_contents_to_claude_messages(contents);
        let system = gemini_system_instruction_to_claude(system_instruction);
        let tool_choice = gemini_tool_choice_to_claude(tool_config);
        let tools = gemini_tools_to_claude(tools);

        let (thinking, output_effort, output_format) =
            claude_thinking_effort_format_from_gemini_generation_config(
                generation_config.as_ref(),
                Some(&model),
            );
        let output_config =
            claude_output_config_from_effort_and_format(output_effort, output_format.clone());

        Ok(ClaudeCountTokensRequest {
            method: HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                messages,
                model,
                context_management: None,
                mcp_servers: None,
                cache_control: None,
                output_config,
                speed: None,
                system,
                thinking,
                tool_choice,
                tools,
            },
        })
    }
}
