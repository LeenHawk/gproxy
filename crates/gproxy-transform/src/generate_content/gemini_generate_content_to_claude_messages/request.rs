use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{config, content, tools};

#[allow(deprecated)] // The public Claude wire still requires writing the legacy slot as absent.
pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: gemini::GenerateContentRequest = serde_json::from_slice(&body)?;
    reject_unsupported(&input)?;
    let max_tokens = input
        .generation_config
        .as_ref()
        .and_then(|config| config.max_output_tokens)
        .and_then(|tokens| u64::try_from(tokens).ok())
        .ok_or_else(|| {
            TransformError::shape("Gemini request", "maxOutputTokens is missing or negative")
        })?;
    let (service_tier, speed) = config::request_tier(input.service_tier);
    let output = claude::CreateMessageRequestBody {
        model: model.to_owned().into(),
        messages: content::request_messages(input.contents)?,
        max_tokens,
        cache_control: None,
        container: None,
        context_management: None,
        diagnostics: None,
        fallback_credit_token: None,
        fallbacks: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: None,
        output_config: config::output(input.generation_config.as_ref())?,
        output_format: None,
        service_tier,
        speed,
        stop_sequences: input
            .generation_config
            .as_ref()
            .and_then(|config| config.stop_sequences.clone()),
        stream: Some(stream),
        system: input
            .system_instruction
            .map(content::system)
            .transpose()?
            .flatten(),
        temperature: input
            .generation_config
            .as_ref()
            .and_then(|config| config.temperature),
        thinking: config::thinking(input.generation_config.as_ref())?,
        tool_choice: tools::choice(input.tool_config)?,
        tools: tools::definitions(input.tools)?,
        top_k: input
            .generation_config
            .as_ref()
            .and_then(|config| config.top_k)
            .map(i64::from),
        top_p: input
            .generation_config
            .as_ref()
            .and_then(|config| config.top_p),
        user_profile_id: None,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn reject_unsupported(input: &gemini::GenerateContentRequest) -> Result<(), TransformError> {
    if input.safety_settings.is_some()
        || input.cached_content.is_some()
        || input.store.is_some()
        || !input.rest.is_empty()
    {
        return Err(TransformError::unsupported(
            "Gemini request",
            "a Gemini-only request parameter",
        ));
    }
    if let Some(config) = &input.tool_config
        && (config.retrieval_config.is_some()
            || config.include_server_side_tool_invocations.is_some()
            || !config.rest.is_empty()
            || config
                .function_calling_config
                .as_ref()
                .is_some_and(|calling| !calling.rest.is_empty()))
    {
        return Err(TransformError::unsupported(
            "Gemini tool config",
            "a non-function tool setting",
        ));
    }
    if let Some(config) = &input.generation_config
        && (config.response_modalities.is_some()
            || config.response_format.is_some()
            || config.speech_config.is_some()
            || config.image_config.is_some()
            || config.candidate_count.is_some()
            || config.seed.is_some()
            || config.presence_penalty.is_some()
            || config.frequency_penalty.is_some()
            || config.response_logprobs.is_some()
            || config.logprobs.is_some()
            || config.enable_enhanced_civic_answers.is_some()
            || config.media_resolution.is_some()
            || config.response_mime_type.is_some()
            || config.private_response_json_schema.is_some()
            || config.thinking_config.as_ref().is_some_and(|thinking| {
                !thinking.rest.is_empty()
                    || (thinking.include_thoughts.is_none()
                        && thinking.thinking_budget.is_none()
                        && thinking.thinking_level.is_none())
            })
            || !config.rest.is_empty())
    {
        return Err(TransformError::unsupported(
            "Gemini generation config",
            "an explicit field without a Claude counterpart",
        ));
    }
    Ok(())
}
