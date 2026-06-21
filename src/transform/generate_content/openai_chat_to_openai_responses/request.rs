use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

use super::content::chat_messages_to_response_items;
use super::tools::{chat_tool_choice_to_response_tool_choice, chat_tools_to_response_tools};

pub fn request(
    input: openai::ChatCompletionRequest,
    _: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    let stream_options = input
        .stream_options
        .map(|options| openai::ResponseStreamOptions {
            include_obfuscation: options.include_obfuscation,
            extra: Default::default(),
        });

    let text = match (input.response_format, input.verbosity) {
        (None, None) => None,
        (format, verbosity) => Some(openai::TextConfig {
            format: format.map(chat_response_format_to_response_format),
            verbosity,
            extra: Default::default(),
        }),
    };

    Ok(openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(
            chat_messages_to_response_items(input.messages),
        )),
        instructions: None,
        max_output_tokens: input.max_completion_tokens.or(input.max_tokens),
        max_tool_calls: None,
        metadata: input.metadata,
        model: Some(input.model),
        moderation: input.moderation,
        parallel_tool_calls: input.parallel_tool_calls,
        previous_response_id: None,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_retention: input.prompt_cache_retention,
        prompt: None,
        reasoning: input
            .reasoning_effort
            .map(|effort| openai::ReasoningConfig {
                effort: Some(effort),
                summary: None,
                generate_summary: None,
                extra: Default::default(),
            }),
        safety_identifier: input.safety_identifier,
        service_tier: input.service_tier,
        store: input.store,
        stream: input.stream,
        stream_options,
        temperature: input.temperature,
        text,
        tool_choice: chat_tool_choice_to_response_tool_choice(input.tool_choice),
        tools: chat_tools_to_response_tools(input.tools),
        top_logprobs: input.top_logprobs,
        top_p: input.top_p,
        truncation: None,
        user: input.user,
        extra: Default::default(),
    })
}

fn chat_response_format_to_response_format(
    format: openai::ChatResponseFormat,
) -> openai::ResponseFormat {
    match format {
        openai::ChatResponseFormat::Text(text) => openai::ResponseFormat::Text(text),
        openai::ChatResponseFormat::JsonObject(json) => openai::ResponseFormat::JsonObject(json),
        openai::ChatResponseFormat::ChatJsonSchema(schema) => {
            openai::ResponseFormat::JsonSchema(openai::JsonSchemaResponseFormat {
                type_: schema.type_,
                name: schema.json_schema.name,
                schema: schema.json_schema.schema.unwrap_or_default(),
                description: schema.json_schema.description,
                strict: schema.json_schema.strict,
                extra: Default::default(),
            })
        }
    }
}
