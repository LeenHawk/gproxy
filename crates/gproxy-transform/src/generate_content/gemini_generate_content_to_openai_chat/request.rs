use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{config, content, tools, wire};

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: gemini::GenerateContentRequest = serde_json::from_slice(&body)?;
    reject_unsupported(&input)?;
    let search = has_search(input.tools.as_ref());
    let mut messages = Vec::new();
    if let Some(system) = input.system_instruction {
        let (content, rest) = content::system_content(system)?;
        messages.push(openai::ChatCompletionMessageParam::System(
            openai::ChatSystemMessageParam {
                role: openai::ChatSystemRole::System,
                content,
                name: None,
                rest,
            },
        ));
    }
    messages.extend(content::messages(input.contents)?);
    let config = config::to_chat(input.generation_config)?;
    let output = openai::ChatCompletionRequest {
        messages,
        model: model.into(),
        audio: None,
        frequency_penalty: config.frequency_penalty,
        function_call: None,
        functions: None,
        logit_bias: None,
        logprobs: config.logprobs,
        max_completion_tokens: config.max_tokens,
        max_tokens: None,
        metadata: None,
        modalities: config.modalities,
        moderation: None,
        n: config.n,
        parallel_tool_calls: None,
        prediction: None,
        presence_penalty: config.presence_penalty,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        reasoning_effort: config.reasoning_effort,
        response_format: config.response_format,
        safety_identifier: None,
        seed: config.seed,
        service_tier: wire::service_tier(input.service_tier)?,
        stop: config.stop,
        store: input.store,
        stream: Some(stream),
        stream_options: stream.then_some(openai::StreamOptions {
            include_obfuscation: None,
            include_usage: Some(true),
            rest: Default::default(),
        }),
        temperature: config.temperature,
        tool_choice: tools::choice(input.tool_config)?,
        tools: tools::transform(input.tools)?,
        top_logprobs: config.top_logprobs,
        top_p: config.top_p,
        user: None,
        verbosity: None,
        web_search_options: search.then_some(openai::ChatWebSearchOptions {
            search_context_size: None,
            user_location: None,
            rest: Default::default(),
        }),
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn has_search(tools: Option<&Vec<gemini::Tool>>) -> bool {
    tools.is_some_and(|tools| {
        tools.iter().any(|tool| {
            tool.google_search.is_some()
                || tool.google_search_retrieval.is_some()
                || tool.url_context.is_some()
        })
    })
}

fn reject_unsupported(input: &gemini::GenerateContentRequest) -> Result<(), TransformError> {
    if input.cached_content.is_some() {
        return Err(TransformError::unsupported(
            "Gemini request",
            "cachedContent has no Chat prompt_cache_key equivalent",
        ));
    }
    if input.safety_settings.is_some() {
        return Err(TransformError::unsupported(
            "Gemini request",
            "safetySettings",
        ));
    }
    if input.tool_config.as_ref().is_some_and(|config| {
        config.retrieval_config.is_some() || config.include_server_side_tool_invocations.is_some()
    }) {
        return Err(TransformError::unsupported(
            "Gemini tool config",
            "retrieval or server-side invocation config",
        ));
    }
    Ok(())
}
