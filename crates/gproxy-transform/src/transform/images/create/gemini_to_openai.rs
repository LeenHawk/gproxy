//! Gemini -> OpenAI create-image transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(
    input: gemini::GenerateContentRequest,
    _: &TransformContext,
) -> Result<openai::ImageGenerationRequest, TransformError> {
    Ok(openai::ImageGenerationRequest {
        prompt: common::gemini_request_prompt(&input),
        background: None,
        model: input.model.map(Into::into),
        moderation: None,
        n: input
            .generation_config
            .as_ref()
            .and_then(|config| config.candidate_count)
            .and_then(common::positive_i32_to_u32),
        output_compression: None,
        output_format: common::gemini_output_format(input.generation_config.as_ref()),
        partial_images: None,
        quality: None,
        response_format: common::gemini_response_format(input.generation_config.as_ref()),
        size: common::gemini_to_openai_create_size(input.generation_config.as_ref()),
        stream: None,
        style: None,
        user: None,
        extra: Default::default(),
    })
}

pub fn response(
    input: gemini::GenerateContentResponse,
    _: &TransformContext,
) -> Result<openai::ImagesResponse, TransformError> {
    Ok(common::gemini_response_to_openai_images(input))
}

pub fn stream_event(
    input: gemini::GenerateContentResponse,
    _: &TransformContext,
) -> Result<openai::ImageGenerationStreamEvent, TransformError> {
    common::gemini_to_openai_generation_stream(input).ok_or_else(|| TransformError::InvalidInput {
        reason: "Gemini image stream chunk did not contain an inline image".to_owned(),
    })
}
