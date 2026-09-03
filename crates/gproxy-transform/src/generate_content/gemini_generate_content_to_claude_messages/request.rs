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
    let output = transform_typed(input, model, stream)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

#[allow(deprecated)] // The public Claude wire still requires writing the legacy slot as absent.
pub(crate) fn transform_typed(
    input: gemini::GenerateContentRequest,
    model: &str,
    stream: bool,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let max_tokens = input
        .generation_config
        .as_ref()
        .and_then(|config| config.max_output_tokens)
        .and_then(|tokens| u64::try_from(tokens).ok())
        .unwrap_or(crate::common::DEFAULT_CLAUDE_MAX_TOKENS);
    let (service_tier, speed) = config::request_tier(input.service_tier);
    let thinking = config::thinking(input.generation_config.as_ref())?;
    let top_k = (!matches!(
        thinking,
        Some(claude::ThinkingConfig::Enabled(_)) | Some(claude::ThinkingConfig::Adaptive(_))
    ))
    .then(|| {
        input
            .generation_config
            .as_ref()
            .and_then(|config| config.top_k)
            .map(i64::from)
    })
    .flatten();
    let output = crate::wire!(claude::CreateMessageRequestBody {
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
        thinking,
        tool_choice: tools::choice(input.tool_config)?,
        tools: tools::definitions(input.tools)?,
        top_k,
        top_p: input
            .generation_config
            .as_ref()
            .and_then(|config| config.top_p),
        user_profile_id: None,
        rest: Default::default(),
    });
    Ok(output)
}
