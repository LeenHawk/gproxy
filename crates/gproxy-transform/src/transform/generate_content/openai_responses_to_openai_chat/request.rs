use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

use super::content::response_input_to_chat_messages;
use super::tools::{response_tool_choice_to_chat_tool_choice, response_tools_for_chat};

pub fn request(
    input: openai::ResponseCreateRequest,
    _: &TransformContext,
) -> Result<openai::ChatCompletionRequest, TransformError> {
    let mut messages = Vec::new();
    if let Some(instructions) = input.instructions {
        messages.push(openai::ChatCompletionMessageParam::Developer {
            content: openai::ChatTextContent::Text(instructions),
            name: None,
            extra: Default::default(),
        });
    }
    messages.extend(response_input_to_chat_messages(input.input));

    let stream_options = input.stream_options.map(|options| openai::StreamOptions {
        include_obfuscation: options.include_obfuscation,
        include_usage: None,
        extra: Default::default(),
    });

    let reasoning_effort = input.reasoning.and_then(|reasoning| reasoning.effort);
    let (response_format, verbosity) = input
        .text
        .map(|text| {
            (
                text.format
                    .and_then(response_format_to_chat_response_format),
                text.verbosity,
            )
        })
        .unwrap_or_default();
    let tools = response_tools_for_chat(input.tools);

    Ok(openai::ChatCompletionRequest {
        messages,
        model: input.model.unwrap_or_else(default_model),
        audio: None,
        frequency_penalty: None,
        function_call: None,
        functions: None,
        logit_bias: None,
        logprobs: None,
        max_completion_tokens: input.max_output_tokens,
        max_tokens: None,
        metadata: input.metadata,
        modalities: None,
        moderation: input.moderation,
        n: None,
        parallel_tool_calls: input.parallel_tool_calls,
        prediction: None,
        presence_penalty: None,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_retention: input.prompt_cache_retention,
        reasoning_effort,
        response_format,
        safety_identifier: input.safety_identifier,
        seed: None,
        service_tier: input.service_tier,
        stop: None,
        store: input.store,
        stream: input.stream,
        stream_options,
        temperature: input.temperature,
        tool_choice: response_tool_choice_to_chat_tool_choice(input.tool_choice),
        tools: tools.tools,
        top_logprobs: input.top_logprobs,
        top_p: input.top_p,
        user: input.user,
        verbosity,
        web_search_options: tools.web_search_options,
        extra: Default::default(),
    })
}

fn response_format_to_chat_response_format(
    format: openai::ResponseFormat,
) -> Option<openai::ChatResponseFormat> {
    Some(match format {
        openai::ResponseFormat::Text(text) => openai::ChatResponseFormat::Text(text),
        openai::ResponseFormat::JsonObject(json) => openai::ChatResponseFormat::JsonObject(json),
        openai::ResponseFormat::JsonSchema(schema) => {
            openai::ChatResponseFormat::ChatJsonSchema(openai::ChatJsonSchemaFormat {
                type_: schema.type_,
                json_schema: openai::JsonSchemaFormat {
                    name: schema.name,
                    description: schema.description,
                    schema: Some(schema.schema),
                    strict: schema.strict,
                    extra: Default::default(),
                },
                extra: Default::default(),
            })
        }
    })
}

fn default_model() -> openai::OpenAiModelId {
    openai::OpenAiModelId::Unknown("unknown".to_owned())
}
