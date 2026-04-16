use crate::claude::create_message::request::{
    ClaudeCreateMessageRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::claude::create_message::types::{HttpMethod, Model};
use crate::gemini::generate_content::request::GeminiGenerateContentRequest;
use crate::transform::gemini::utils::{
    claude_output_config_from_effort_and_format,
    claude_thinking_effort_format_from_gemini_generation_config,
    gemini_contents_to_claude_messages, gemini_system_instruction_to_claude,
    gemini_tool_choice_to_claude, gemini_tools_to_claude, strip_models_prefix,
};
use crate::transform::utils::TransformError;

impl TryFrom<GeminiGenerateContentRequest> for ClaudeCreateMessageRequest {
    type Error = TransformError;

    fn try_from(value: GeminiGenerateContentRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let model = Model::Custom(strip_models_prefix(&value.path.model));
        let messages = gemini_contents_to_claude_messages(body.contents);
        let system = gemini_system_instruction_to_claude(body.system_instruction);
        let tool_choice = gemini_tool_choice_to_claude(body.tool_config);
        let tools = gemini_tools_to_claude(body.tools);

        let generation_config = body.generation_config;
        let max_tokens = generation_config
            .as_ref()
            .and_then(|config| config.max_output_tokens)
            .map(u64::from)
            .unwrap_or(8192);
        let stop_sequences = generation_config
            .as_ref()
            .and_then(|config| config.stop_sequences.clone());
        let temperature = generation_config
            .as_ref()
            .and_then(|config| config.temperature);
        let top_k = generation_config
            .as_ref()
            .and_then(|config| config.top_k)
            .map(u64::from);
        let top_p = generation_config.as_ref().and_then(|config| config.top_p);

        let (thinking, output_effort, output_format) =
            claude_thinking_effort_format_from_gemini_generation_config(
                generation_config.as_ref(),
                Some(&model),
            );
        let output_config =
            claude_output_config_from_effort_and_format(output_effort, output_format.clone());

        Ok(ClaudeCreateMessageRequest {
            method: HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                max_tokens,
                messages,
                model,
                container: None,
                context_management: None,
                inference_geo: None,
                mcp_servers: None,
                metadata: None,
                cache_control: None,
                output_config,
                output_format,
                service_tier: None,
                speed: None,
                stop_sequences,
                stream: None,
                system,
                temperature,
                thinking,
                tool_choice,
                tools,
                top_k,
                top_p,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::count_tokens::types as ct;
    use crate::gemini::count_tokens::types::{GeminiContentRole, GeminiPart};
    use crate::gemini::generate_content::request::{
        GeminiGenerateContentRequest, PathParameters as GeminiPathParameters,
        QueryParameters as GeminiQueryParameters, RequestBody as GeminiRequestBody,
        RequestHeaders as GeminiRequestHeaders,
    };
    use crate::gemini::generate_content::types::{
        GeminiContent, GeminiGenerationConfig, GeminiThinkingConfig,
    };

    #[test]
    fn opus_47_converts_budgeted_gemini_thinking_to_adaptive() {
        let request = GeminiGenerateContentRequest {
            method: crate::gemini::types::HttpMethod::Post,
            path: GeminiPathParameters {
                model: "models/claude-opus-4-7".to_string(),
            },
            query: GeminiQueryParameters::default(),
            headers: GeminiRequestHeaders::default(),
            body: GeminiRequestBody {
                contents: vec![GeminiContent {
                    parts: vec![GeminiPart {
                        text: Some("hi".to_string()),
                        ..Default::default()
                    }],
                    role: Some(GeminiContentRole::User),
                }],
                generation_config: Some(GeminiGenerationConfig {
                    thinking_config: Some(GeminiThinkingConfig {
                        thinking_budget: Some(4_096),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        let claude_request = ClaudeCreateMessageRequest::try_from(request).expect("transform");
        assert!(matches!(
            claude_request.body.thinking,
            Some(ct::BetaThinkingConfigParam::Adaptive(_))
        ));
    }
}
