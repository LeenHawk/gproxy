mod content;
mod messages;

use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::tools;

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionRequest = serde_json::from_slice(&body)?;
    let mut items = Vec::new();
    for message in input.messages {
        items.extend(messages::message_items(message)?);
    }
    let output = openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(items)),
        instructions: None,
        max_output_tokens: input.max_completion_tokens.or(input.max_tokens),
        max_tool_calls: None,
        metadata: input.metadata,
        model: Some(model.into()),
        moderation: input.moderation,
        multi_agent: None,
        parallel_tool_calls: input.parallel_tool_calls,
        previous_response_id: None,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        prompt: None,
        reasoning: input
            .reasoning_effort
            .map(|effort| openai::ReasoningConfig {
                context: None,
                effort: Some(effort),
                mode: None,
                summary: None,
                generate_summary: None,
                rest: Default::default(),
            }),
        safety_identifier: input.safety_identifier,
        service_tier: input.service_tier,
        store: input.store,
        stream: Some(stream),
        stream_options: input
            .stream_options
            .map(|options| openai::ResponseStreamOptions {
                include_obfuscation: options.include_obfuscation,
                rest: options.rest,
            }),
        temperature: input.temperature,
        text: text_config(input.response_format, input.verbosity)?,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::chat_to_responses(input.tools)?,
        top_logprobs: input.top_logprobs,
        top_p: input.top_p,
        truncation: None,
        user: input.user,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn tool_choice(
    choice: Option<openai::ChatToolChoice>,
) -> Result<Option<openai::ResponseToolChoice>, TransformError> {
    Ok(match choice {
        None => None,
        Some(openai::ChatToolChoice::Mode(mode)) => Some(openai::ResponseToolChoice::Mode(mode)),
        Some(openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Function(choice))) => Some(
            openai::ResponseToolChoice::Function(openai::ResponseFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                name: choice.function.name,
                rest: choice.rest,
            }),
        ),
        Some(openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Custom(choice))) => Some(
            openai::ResponseToolChoice::Custom(openai::ResponseCustomToolChoice {
                type_: openai::CustomToolChoiceType::Custom,
                name: choice.custom.name,
                rest: choice.rest,
            }),
        ),
        Some(openai::ChatToolChoice::Unknown(raw)) => {
            Some(openai::ResponseToolChoice::Unknown(raw))
        }
        Some(other) => serde_json::from_slice(&serde_json::to_vec(&other)?).map(Some)?,
    })
}

fn text_config(
    format: Option<openai::ChatResponseFormat>,
    verbosity: Option<openai::Verbosity>,
) -> Result<Option<openai::TextConfig>, TransformError> {
    let format = format
        .map(|format| serde_json::from_slice(&serde_json::to_vec(&format)?))
        .transpose()?;
    Ok(
        (format.is_some() || verbosity.is_some()).then_some(openai::TextConfig {
            format,
            verbosity,
            rest: Default::default(),
        }),
    )
}
