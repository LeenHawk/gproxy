use crate::protocol::{claude, openai};
use crate::transform::generate_content::common::cache::{
    claude_prompt_cache_key, openai_options_for_claude_root,
};
use crate::transform::{TransformContext, TransformError};

use super::input::{claude_messages_to_openai_items, system_to_openai_item};
use super::util::{
    claude_previous_message_id_to_openai, claude_service_tier_to_compact, claude_system_to_text,
    model_to_string,
};

pub fn request(
    input: claude::CreateMessageRequestBody,
    _: &TransformContext,
) -> Result<openai::CompactResponseRequestBody, TransformError> {
    let prompt_cache_key = claude_prompt_cache_key(&input);
    let compact_instructions = compact_instructions(input.context_management.as_ref());
    let system = input.system.and_then(claude_system_to_text);
    let mut input_items = claude_messages_to_openai_items(input.messages);
    if compact_instructions.is_some()
        && let Some(system) = system.as_ref()
    {
        input_items.insert(0, system_to_openai_item(system.clone()));
    }

    Ok(openai::CompactResponseRequestBody {
        input: Some(openai::ResponseInput::Items(input_items)),
        instructions: compact_instructions.or(system),
        model: openai::OpenAiModelId::Unknown(model_to_string(&input.model)),
        previous_response_id: claude_previous_message_id_to_openai(input.diagnostics),
        prompt_cache_key: Some(prompt_cache_key),
        prompt_cache_options: openai_options_for_claude_root(input.cache_control),
        prompt_cache_retention: None,
        service_tier: claude_service_tier_to_compact(input.service_tier),
        extra: Default::default(),
    })
}

fn compact_instructions(
    context_management: Option<&claude::ContextManagementConfig>,
) -> Option<String> {
    context_management
        .and_then(|context| context.edits.as_ref())
        .and_then(|edits| {
            edits.iter().find_map(|edit| match edit {
                claude::ContextEdit::Known(claude::KnownContextEdit::Compact {
                    instructions,
                    ..
                }) => instructions.clone(),
                _ => None,
            })
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{Operation, OperationKey, Provider};

    #[test]
    fn compact_reuses_claude_session_cache_key_and_root_policy() {
        let input = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "cache_control": {"type": "ephemeral"},
            "metadata": {
                "user_id": "{\"session_id\":\"session-compact\"}"
            },
            "messages": [{"role": "user", "content": "compact this"}]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            OperationKey::provider(Operation::CompactContent, Provider::Claude),
            OperationKey::provider(Operation::CompactContent, Provider::OpenAi),
        );

        let output = request(input, &ctx).unwrap();

        assert_eq!(output.prompt_cache_key.as_deref(), Some("session-compact"));
        assert_eq!(
            output
                .prompt_cache_options
                .as_ref()
                .and_then(|options| options.mode.as_ref()),
            Some(&openai::PromptCacheMode::Implicit)
        );
    }
}
