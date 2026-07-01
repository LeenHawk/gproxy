//! OpenAI -> Gemini create-image transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(
    input: openai::ImageGenerationRequest,
    _: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    let model = common::openai_model_string(input.model);
    let size = common::create_size_to_shape(input.size);

    Ok(gemini::GenerateContentRequest {
        model,
        contents: vec![common::prompt_content(input.prompt, Vec::new())],
        tools: Vec::new(),
        tool_config: None,
        safety_settings: Vec::new(),
        system_instruction: None,
        generation_config: Some(common::generation_config(
            input.n,
            size,
            input.output_format,
            input.response_format,
        )),
        cached_content: None,
        service_tier: None,
        store: None,
        extra: Default::default(),
    })
}

pub fn response(
    input: openai::ImagesResponse,
    _: &TransformContext,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    Ok(common::openai_images_response_to_gemini(input))
}

pub fn stream_event(
    input: openai::ImageGenerationStreamEvent,
    _: &TransformContext,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    common::openai_generation_stream_to_gemini(input).ok_or_else(|| TransformError::InvalidInput {
        reason: "unknown OpenAI image generation stream event cannot be converted".to_owned(),
    })
}
