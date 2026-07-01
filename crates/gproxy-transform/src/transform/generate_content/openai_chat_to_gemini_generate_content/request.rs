use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::{chat_messages_to_gemini, text_content_to_gemini_content};
use super::tools::{chat_tool_choice_to_gemini, chat_tools_to_gemini};

pub fn request(
    input: openai::ChatCompletionRequest,
    _: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    let max_output_tokens =
        common::merge_openai_max_tokens(input.max_completion_tokens, input.max_tokens);
    let (contents, system_instruction) = chat_messages_to_gemini(input.messages);
    let generation_config = generation_config(
        input.stop,
        max_output_tokens,
        input.response_format,
        input.reasoning_effort,
        input.n,
        input.temperature,
        input.top_p,
        input.seed,
        input.presence_penalty,
        input.frequency_penalty,
        input.logprobs,
        input.top_logprobs,
    );

    let mut tools = input.tools.map(chat_tools_to_gemini).unwrap_or_default();
    if input.web_search_options.is_some() {
        tools.push(gemini::Tool {
            google_search: Some(gemini::GoogleSearch::default()),
            ..Default::default()
        });
    }

    Ok(gemini::GenerateContentRequest {
        model: Some(common::openai_model_string(input.model)),
        contents,
        tools,
        tool_config: chat_tool_choice_to_gemini(input.tool_choice),
        safety_settings: Vec::new(),
        system_instruction,
        generation_config,
        cached_content: input.prompt_cache_key,
        service_tier: common::openai_service_tier_to_gemini(input.service_tier),
        store: input.store,
        extra: Default::default(),
    })
}

#[allow(clippy::too_many_arguments)]
fn generation_config(
    stop: Option<openai::StringOrList>,
    max_output_tokens: Option<u32>,
    response_format: Option<openai::ChatResponseFormat>,
    reasoning_effort: Option<openai::ReasoningEffort>,
    candidate_count: Option<u32>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    seed: Option<i64>,
    presence_penalty: Option<f64>,
    frequency_penalty: Option<f64>,
    logprobs: Option<bool>,
    top_logprobs: Option<u32>,
) -> Option<gemini::GenerationConfig> {
    let mut config = gemini::GenerationConfig {
        stop_sequences: common::openai_stop_to_vec(stop).unwrap_or_default(),
        response_mime_type: common::chat_response_format_to_gemini(response_format.clone()),
        response_json_schema: common::response_format_to_gemini_schema(response_format),
        candidate_count: candidate_count.map(u32_to_i32),
        max_output_tokens: max_output_tokens.map(u32_to_i32),
        temperature,
        top_p,
        seed,
        presence_penalty,
        frequency_penalty,
        response_logprobs: logprobs,
        logprobs: top_logprobs.map(u32_to_i32),
        thinking_config: common::openai_reasoning_to_gemini(reasoning_effort),
        ..Default::default()
    };

    if config.response_mime_type.is_some() && config.response_json_schema.is_some() {
        config.response_mime_type = Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::ApplicationJson,
        ));
    }

    (config != gemini::GenerationConfig::default()).then_some(config)
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[allow(dead_code)]
fn text_system_instruction(text: Option<String>) -> Option<gemini::Content> {
    text.filter(|value| !value.is_empty()).map(|text| {
        text_content_to_gemini_content(
            text,
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
        )
    })
}
