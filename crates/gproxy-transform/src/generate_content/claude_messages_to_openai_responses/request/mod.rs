mod config;
mod items;
mod tool_output;

use std::collections::BTreeMap;

use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::tools;

use config::{parallel, reasoning, service_tier, system_text, text_config, tool_choice};
use items::message_items;

#[allow(deprecated)] // Reading the legacy Claude output_format remains necessary on the wire.
pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageRequestBody = serde_json::from_slice(&body)?;
    if input.cache_control.is_some()
        || input.container.is_some()
        || input.context_management.is_some()
        || input.fallback_credit_token.is_some()
        || input.fallbacks.is_some()
        || input.inference_geo.is_some()
        || input.mcp_servers.is_some()
        || input.stop_sequences.is_some()
        || input.top_k.is_some()
        || input.user_profile_id.is_some()
    {
        return Err(TransformError::unsupported(
            "Claude request",
            "an unmodeled Claude-only request parameter",
        ));
    }
    let mut response_items = Vec::new();
    let mut native_calls = BTreeMap::new();
    for message in input.messages {
        response_items.extend(message_items(message, &mut native_calls)?);
    }
    let output = openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(response_items)),
        instructions: input.system.map(system_text).transpose()?,
        max_output_tokens: Some(input.max_tokens.min(u64::from(u32::MAX)) as u32),
        max_tool_calls: None,
        metadata: input.metadata.and_then(|metadata| {
            metadata.user_id.map(|user_id| {
                [("user_id".into(), user_id)]
                    .into_iter()
                    .collect::<openai::Metadata>()
            })
        }),
        model: Some(model.into()),
        moderation: None,
        multi_agent: None,
        parallel_tool_calls: parallel(&input.tool_choice),
        previous_response_id: input
            .diagnostics
            .and_then(|diagnostics| diagnostics.previous_message_id.flatten()),
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        prompt: None,
        reasoning: reasoning(input.output_config.as_ref(), input.thinking.as_ref())?,
        safety_identifier: None,
        service_tier: service_tier(input.service_tier, input.speed)?,
        store: None,
        stream: Some(stream),
        stream_options: None,
        temperature: input.temperature,
        text: text_config(input.output_config.as_ref(), input.output_format.as_ref())?,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::claude_to_responses(input.tools)?,
        top_logprobs: None,
        top_p: input.top_p,
        truncation: None,
        user: None,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

#[allow(deprecated)]
pub(crate) fn count_tokens(
    input: claude::CountTokensRequestBody,
    model: &str,
) -> Result<openai::ResponseInputTokensRequest, TransformError> {
    if input.cache_control.is_some()
        || input.context_management.is_some()
        || input.mcp_servers.is_some()
    {
        return Err(TransformError::unsupported(
            "Claude count-tokens request",
            "an unmodeled Claude-only request parameter",
        ));
    }
    let mut response_items = Vec::new();
    let mut native_calls = BTreeMap::new();
    for message in input.messages {
        response_items.extend(message_items(message, &mut native_calls)?);
    }
    Ok(openai::ResponseInputTokensRequest {
        conversation: None,
        input: Some(openai::ResponseInput::Items(response_items)),
        instructions: input.system.map(system_text).transpose()?,
        model: Some(model.into()),
        parallel_tool_calls: parallel(&input.tool_choice),
        personality: None,
        previous_response_id: input
            .diagnostics
            .and_then(|diagnostics| diagnostics.previous_message_id.flatten()),
        prompt_cache_options: None,
        reasoning: reasoning(input.output_config.as_ref(), input.thinking.as_ref())?,
        service_tier: service_tier(input.service_tier, input.speed)?,
        text: text_config(input.output_config.as_ref(), input.output_format.as_ref())?,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::claude_to_responses(input.tools)?,
        truncation: None,
        rest: input.rest,
    })
}
