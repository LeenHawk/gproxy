use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes, model: &str) -> Result<bytes::Bytes, TransformError> {
    let input: openai::CompactResponseRequestBody = serde_json::from_slice(&body)?;
    let instructions = input.instructions.clone();
    let request = openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: input.input,
        instructions: input.instructions,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: input.model.or_else(|| Some(model.into())),
        moderation: None,
        multi_agent: None,
        parallel_tool_calls: None,
        previous_response_id: input.previous_response_id,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        prompt: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: input.service_tier,
        store: None,
        stream: Some(false),
        stream_options: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        user: None,
        rest: input.rest,
    };
    let converted =
        crate::generate_content::openai_responses_to_claude_messages::request::transform(
            bytes::Bytes::from(serde_json::to_vec(&request)?),
            model,
            false,
        )?;
    let mut output: claude::CreateMessageRequestBody = serde_json::from_slice(&converted)?;
    output.context_management = Some(claude::ContextManagementConfig {
        edits: Some(vec![claude::ContextEdit::Known(
            claude::KnownContextEdit::Compact {
                instructions,
                pause_after_compaction: Some(true),
                trigger: None,
                rest: Default::default(),
            },
        )]),
        rest: Default::default(),
    });
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
