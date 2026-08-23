use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{config, content, tools};

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    _stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageRequestBody = serde_json::from_slice(&body)?;
    reject_unsupported(&input)?;
    let generation_config = config::generation(&input)?;
    let tool_config = tools::choice(input.tool_choice);
    let output = gemini::GenerateContentRequest {
        model: Some(model.to_owned()),
        contents: content::request_messages(input.messages)?,
        tools: {
            let tools = tools::definitions(input.tools)?;
            (!tools.is_empty()).then_some(tools)
        },
        tool_config,
        safety_settings: None,
        system_instruction: content::system(input.system)?,
        generation_config: Some(generation_config),
        cached_content: None,
        service_tier: config::request_tier(input.speed, input.service_tier),
        store: None,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

#[allow(deprecated)] // Validation must also cover the legacy Claude output_format slot.
fn reject_unsupported(input: &claude::CreateMessageRequestBody) -> Result<(), TransformError> {
    if input.cache_control.is_some()
        || input.container.is_some()
        || input.context_management.is_some()
        || input.diagnostics.is_some()
        || input.fallback_credit_token.is_some()
        || input.fallbacks.is_some()
        || input.inference_geo.is_some()
        || input.mcp_servers.is_some()
        || input.metadata.is_some()
        || input.user_profile_id.is_some()
        || !input.rest.is_empty()
        || input.output_config.as_ref().is_some_and(|config| {
            config.task_budget.is_some()
                || !config.rest.is_empty()
                || config
                    .format
                    .as_ref()
                    .is_some_and(|format| !format.rest.is_empty())
        })
        || input
            .output_format
            .as_ref()
            .is_some_and(|format| !format.rest.is_empty())
        || input.tool_choice.as_ref().is_some_and(unmapped_choice)
        || input.thinking.as_ref().is_some_and(unmapped_thinking)
    {
        return Err(TransformError::unsupported(
            "Claude request",
            "a Claude-only request parameter",
        ));
    }
    Ok(())
}

fn unmapped_choice(choice: &claude::ToolChoice) -> bool {
    match choice {
        claude::ToolChoice::Auto(choice) => {
            choice.disable_parallel_tool_use.is_some() || !choice.rest.is_empty()
        }
        claude::ToolChoice::Any(choice) => {
            choice.disable_parallel_tool_use.is_some() || !choice.rest.is_empty()
        }
        claude::ToolChoice::Tool(choice) => {
            choice.disable_parallel_tool_use.is_some() || !choice.rest.is_empty()
        }
        claude::ToolChoice::None(choice) => !choice.rest.is_empty(),
        claude::ToolChoice::Unknown(_) => true,
        _ => true,
    }
}

fn unmapped_thinking(thinking: &claude::ThinkingConfig) -> bool {
    match thinking {
        claude::ThinkingConfig::Enabled(config) => {
            config.display.is_some() || !config.rest.is_empty()
        }
        claude::ThinkingConfig::Adaptive(config) => {
            config.display.is_some() || !config.rest.is_empty()
        }
        claude::ThinkingConfig::Disabled(config) => !config.rest.is_empty(),
        claude::ThinkingConfig::Unknown(_) => true,
        _ => true,
    }
}
