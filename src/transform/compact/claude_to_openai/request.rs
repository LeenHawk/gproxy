use crate::protocol::{claude, openai};
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
        prompt_cache_key: None,
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
