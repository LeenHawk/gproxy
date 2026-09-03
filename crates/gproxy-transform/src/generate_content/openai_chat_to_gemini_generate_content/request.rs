use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{config, content, tools, wire};

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    _stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionRequest = serde_json::from_slice(&body)?;
    let output = transform_typed(input, model, _stream)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::ChatCompletionRequest,
    model: &str,
    _stream: bool,
) -> Result<gemini::GenerateContentRequest, TransformError> {
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
    let output = crate::wire!(gemini::GenerateContentRequest {
        model: Some(model.into()),
        contents,
        tools: tools::transform(input.tools, input.web_search_options.is_some())?,
        tool_config: tools::choice(input.tool_choice)?,
        safety_settings: None,
        system_instruction,
        generation_config,
        cached_content: input.prompt_cache_key,
        service_tier: wire::service_tier(input.service_tier),
        store: input.store,
        rest: Default::default(),
    });
    Ok(output)
}
