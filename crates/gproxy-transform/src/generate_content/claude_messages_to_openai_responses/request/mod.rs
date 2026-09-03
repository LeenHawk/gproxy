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
    let output = transform_typed(input, model, stream)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

#[allow(deprecated)] // Reading the legacy Claude output_format remains necessary on the wire.
pub(crate) fn transform_typed(
    mut input: claude::CreateMessageRequestBody,
    model: &str,
    stream: bool,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    crate::common::claude_message_controls::apply(&mut input.messages, &mut input.output_config);
    let mut response_items = system_items(input.system);
    let mut native_calls = BTreeMap::new();
    for message in input.messages {
        response_items.extend(message_items(message, &mut native_calls)?);
    }
    let output = crate::wire!(openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(response_items)),
        instructions: None,
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
        prompt_cache_options: Some(openai::PromptCacheOptions {
            mode: Some(openai::PromptCacheMode::Implicit),
            ttl: input
                .cache_control
                .map(|_| openai::PromptCacheTtl::ThirtyMinutes),
            rest: Default::default(),
        }),
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
        tools: response_tools(input.tools, input.mcp_servers)?,
        top_logprobs: None,
        top_p: input.top_p,
        truncation: None,
        user: None,
        rest: Default::default(),
    });
    Ok(output)
}

fn response_tools(
    tools: Option<Vec<claude::Tool>>,
    servers: Option<Vec<claude::McpServer>>,
) -> Result<Option<Vec<openai::ResponseTool>>, TransformError> {
    let mut output = tools::claude_to_responses(tools)?.unwrap_or_default();
    for server in servers.into_iter().flatten() {
        let allowed_tools = server
            .tool_configuration
            .and_then(|config| config.allowed_tools.map(openai::McpAllowedTools::Names));
        output.push(openai::ResponseTool::Mcp {
            server_label: server.name,
            allowed_tools,
            authorization: server.authorization_token,
            connector_id: None,
            defer_loading: None,
            headers: None,
            require_approval: None,
            server_description: None,
            server_url: Some(server.url),
            tunnel_id: None,
            allowed_callers: None,
            rest: Default::default(),
        });
    }
    Ok((!output.is_empty()).then_some(output))
}

fn system_items(system: Option<claude::SystemPrompt>) -> Vec<openai::ResponseItem> {
    let blocks = match system {
        Some(claude::StringOrArray::String(text)) => {
            vec![crate::wire!(claude::TextBlock {
                text,
                type_: claude::TextBlockType::Text,
                cache_control: None,
                citations: None,
                rest: Default::default(),
            })]
        }
        Some(claude::StringOrArray::Array(blocks)) => blocks,
        Some(claude::StringOrArray::Raw(_)) => return Vec::new(),
        _future => return Vec::new(),
    };
    vec![openai::ResponseItem::Message(
        openai::ResponseMessageItem::EasyInput(crate::wire!(
            openai::ResponseEasyInputMessageItem {
                type_: Some(openai::ResponseMessageItemType::Message),
                role: openai::ResponseEasyInputMessageRole::System,
                content: openai::ResponseEasyInputContent::Parts(
                    blocks
                        .into_iter()
                        .map(|block| {
                            openai::ResponseInputContentPart::InputText(openai::ResponseInputText {
                                text: block.text,
                                prompt_cache_breakpoint: block.cache_control.map(|_| {
                                    openai::PromptCacheBreakpoint {
                                        mode: openai::PromptCacheBreakpointMode::Explicit,
                                        rest: Default::default(),
                                    }
                                }),
                                rest: Default::default(),
                            })
                        })
                        .collect(),
                ),
                phase: None,
                rest: Default::default(),
            }
        )),
    )]
}

#[allow(deprecated)]
pub(crate) fn count_tokens(
    mut input: claude::CountTokensRequestBody,
    model: &str,
) -> Result<openai::ResponseInputTokensRequest, TransformError> {
    crate::common::claude_message_controls::apply(&mut input.messages, &mut input.output_config);
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
    Ok(crate::wire!(openai::ResponseInputTokensRequest {
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
        rest: Default::default(),
    }))
}
