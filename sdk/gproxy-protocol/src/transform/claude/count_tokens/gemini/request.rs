use crate::claude::count_tokens::request::ClaudeCountTokensRequest;
use crate::claude::count_tokens::types::BetaMessageRole;
use crate::gemini::count_tokens::request::{
    GeminiCountTokensRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::gemini::count_tokens::types::{
    GeminiContent, GeminiContentRole, GeminiGenerateContentRequest, GeminiGenerationConfig,
    GeminiPart, HttpMethod,
};
use crate::transform::claude::count_tokens::utils::{
    beta_message_content_to_text, claude_model_to_string,
};
use crate::transform::claude::generate_content::gemini::utils::{
    gemini_system_instruction_from_claude, gemini_thinking_config_from_claude,
    gemini_tool_config_from_claude, gemini_tools_from_claude,
};
use crate::transform::claude::model_list::gemini::utils::ensure_models_prefix;
use crate::transform::utils::TransformError;

impl TryFrom<ClaudeCountTokensRequest> for GeminiCountTokensRequest {
    type Error = TransformError;

    fn try_from(value: ClaudeCountTokensRequest) -> Result<Self, TransformError> {
        let model = ensure_models_prefix(&claude_model_to_string(&value.body.model));
        let contents = value
            .body
            .messages
            .into_iter()
            .map(|message| GeminiContent {
                parts: vec![GeminiPart {
                    text: Some(beta_message_content_to_text(&message.content)),
                    ..GeminiPart::default()
                }],
                role: Some(match message.role {
                    BetaMessageRole::User => GeminiContentRole::User,
                    BetaMessageRole::Assistant => GeminiContentRole::Model,
                }),
            })
            .collect::<Vec<_>>();
        let tools = gemini_tools_from_claude(value.body.tools, false);
        let tool_config = gemini_tool_config_from_claude(value.body.tool_choice);
        let thinking_config = gemini_thinking_config_from_claude(
            value.body.thinking,
            value
                .body
                .output_config
                .as_ref()
                .and_then(|config| config.effort.as_ref()),
        );
        let system_instruction = gemini_system_instruction_from_claude(value.body.system);
        let json_output_requested = value
            .body
            .output_config
            .as_ref()
            .and_then(|config| config.format.as_ref())
            .is_some();
        let generation_config = if thinking_config.is_some() || json_output_requested {
            Some(GeminiGenerationConfig {
                response_mime_type: if json_output_requested {
                    Some("application/json".to_string())
                } else {
                    None
                },
                thinking_config,
                ..GeminiGenerationConfig::default()
            })
        } else {
            None
        };

        Ok(Self {
            method: HttpMethod::Post,
            path: PathParameters {
                model: model.clone(),
            },
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                contents: None,
                generate_content_request: Some(GeminiGenerateContentRequest {
                    model,
                    contents,
                    tools,
                    tool_config,
                    safety_settings: None,
                    system_instruction,
                    generation_config,
                    cached_content: None,
                }),
            },
        })
    }
}
