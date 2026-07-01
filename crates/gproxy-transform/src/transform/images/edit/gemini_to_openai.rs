//! Gemini -> OpenAI edit-image transforms.

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn request(
    input: gemini::GenerateContentRequest,
    _: &TransformContext,
) -> Result<openai::ImageEditRequest, TransformError> {
    let images = common::gemini_request_image_references(&input);
    if images.is_empty() {
        return Err(TransformError::InvalidInput {
            reason: "Gemini edit-image request did not contain image parts".to_owned(),
        });
    }

    Ok(openai::ImageEditRequest {
        images,
        prompt: common::gemini_request_prompt(&input),
        background: None,
        input_fidelity: None,
        mask: None,
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
        size: common::gemini_to_openai_edit_size(input.generation_config.as_ref()),
        stream: None,
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
) -> Result<openai::ImageEditStreamEvent, TransformError> {
    common::gemini_to_openai_edit_stream(input).ok_or_else(|| TransformError::InvalidInput {
        reason: "Gemini image stream chunk did not contain an inline image".to_owned(),
    })
}
