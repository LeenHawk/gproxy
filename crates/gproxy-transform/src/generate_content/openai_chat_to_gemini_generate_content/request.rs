use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{config, content, tools, wire};

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    _stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionRequest = serde_json::from_slice(&body)?;
    reject_unsupported(&input)?;
    let (contents, system_instruction) = content::messages(input.messages)?;
    let generation_config = config::to_gemini(config::Input {
        audio: input.audio,
        frequency_penalty: input.frequency_penalty,
        logprobs: input.logprobs,
        max_tokens: input.max_completion_tokens.or(input.max_tokens),
        modalities: input.modalities,
        n: input.n,
        presence_penalty: input.presence_penalty,
        reasoning: input.reasoning_effort,
        response_format: input.response_format,
        seed: input.seed,
        stop: input.stop,
        temperature: input.temperature,
        top_logprobs: input.top_logprobs,
        top_p: input.top_p,
    })?;
    let output = gemini::GenerateContentRequest {
        model: Some(model.into()),
        contents,
        tools: tools::transform(input.tools, input.web_search_options.is_some())?,
        tool_config: tools::choice(input.tool_choice)?,
        safety_settings: None,
        system_instruction,
        generation_config,
        cached_content: None,
        service_tier: wire::service_tier(input.service_tier),
        store: input.store,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn reject_unsupported(input: &openai::ChatCompletionRequest) -> Result<(), TransformError> {
    if input.prompt_cache_key.is_some() {
        return Err(TransformError::unsupported(
            "Chat request",
            "prompt_cache_key has no Gemini cachedContent equivalent",
        ));
    }
    if input.function_call.is_some()
        || input.functions.is_some()
        || input.logit_bias.is_some()
        || input.metadata.is_some()
        || input.moderation.is_some()
        || input.parallel_tool_calls.is_some()
        || input.prediction.is_some()
        || input.prompt_cache_options.is_some()
        || input.prompt_cache_retention.is_some()
        || input.safety_identifier.is_some()
        || input.user.is_some()
        || input.verbosity.is_some()
    {
        return Err(TransformError::unsupported(
            "Chat request",
            "an option with no Gemini counterpart",
        ));
    }
    if input.web_search_options.as_ref().is_some_and(|options| {
        options.search_context_size.is_some() || options.user_location.is_some()
    }) {
        return Err(TransformError::unsupported(
            "Chat web search options",
            "search context size or user location",
        ));
    }
    Ok(())
}
