use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::DEFAULT_COMPACT_MAX_TOKENS;
use super::input::openai_input_to_claude_messages;
use super::util::{
    compact_service_tier_to_claude, model_to_string, openai_previous_response_id_to_claude,
};

pub fn request_headers(_: &TransformContext) -> claude::CreateMessageRequestHeaders {
    claude::CreateMessageRequestHeaders {
        anthropic_beta: Some(vec![claude::AnthropicBeta::Known(
            claude::AnthropicBetaKnown::ContextManagement20250627,
        )]),
        extra: Default::default(),
    }
}

pub fn request(
    input: openai::CompactResponseRequestBody,
    _: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    #[allow(deprecated)]
    Ok(claude::CreateMessageRequestBody {
        model: claude::ClaudeModel::Unknown(model_to_string(&input.model)),
        messages: openai_input_to_claude_messages(input.input),
        max_tokens: DEFAULT_COMPACT_MAX_TOKENS,
        cache_control: None,
        container: None,
        context_management: Some(compact_context_management(input.instructions.as_deref())),
        diagnostics: openai_previous_response_id_to_claude(input.previous_response_id),
        fallback_credit_token: None,
        fallbacks: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: None,
        output_config: None,
        output_format: None,
        service_tier: compact_service_tier_to_claude(input.service_tier),
        speed: None,
        stop_sequences: None,
        stream: None,
        system: input.instructions.map(claude::SystemPrompt::String),
        temperature: None,
        thinking: None,
        tool_choice: None,
        tools: None,
        top_k: None,
        top_p: None,
        user_profile_id: None,
        extra: Default::default(),
    })
}

fn compact_context_management(instructions: Option<&str>) -> claude::ContextManagementConfig {
    claude::ContextManagementConfig {
        edits: Some(vec![claude::ContextEdit::Known(
            claude::KnownContextEdit::Compact {
                instructions: instructions.map(str::to_owned),
                pause_after_compaction: Some(true),
                trigger: None,
                extra: Default::default(),
            },
        )]),
        extra: Default::default(),
    }
}
