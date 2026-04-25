use crate::openai::count_tokens::types as ot;
use crate::openai::create_image_edit::request::{
    OpenAiCreateImageEditRequest, RequestBody as CreateImageEditRequestBody,
};
use crate::openai::create_image_edit::types as iet;
use crate::openai::create_response::request::{
    OpenAiCreateResponseRequest, RequestBody as ResponseRequestBody,
};
use crate::transform::openai::create_image::utils::{
    create_image_edit_model_to_string, image_tool_choice, response_image_background_from_request,
    response_image_model_from_create_image_edit_model, response_image_moderation_from_request,
    response_image_output_format_from_request,
    response_image_quality_from_create_image_edit_request,
    response_image_size_from_create_image_edit_request, user_message_from_parts,
};
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiCreateImageEditRequest> for OpenAiCreateResponseRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiCreateImageEditRequest) -> Result<Self, TransformError> {
        OpenAiCreateResponseRequest::try_from(&value)
    }
}

impl TryFrom<&OpenAiCreateImageEditRequest> for OpenAiCreateResponseRequest {
    type Error = TransformError;

    fn try_from(value: &OpenAiCreateImageEditRequest) -> Result<Self, TransformError> {
        create_image_edit_to_response_request(&value.body)
    }
}

fn create_image_edit_to_response_request(
    body: &CreateImageEditRequestBody,
) -> Result<OpenAiCreateResponseRequest, TransformError> {
    // Build input: image references + text prompt
    let mut parts = Vec::with_capacity(body.images.len() + 1);
    for image in &body.images {
        parts.push(ot::ResponseInputContent::Image(ot::ResponseInputImage {
            type_: ot::ResponseInputImageType::InputImage,
            detail: None,
            file_id: image.file_id.clone(),
            image_url: image.image_url.clone(),
        }));
    }
    parts.push(ot::ResponseInputContent::Text(ot::ResponseInputText {
        text: body.prompt.clone(),
        type_: ot::ResponseInputTextType::InputText,
    }));

    let input = user_message_from_parts(parts);

    // Build the image_generation tool with edit action
    let image_tool = ot::ResponseImageGenerationTool {
        type_: ot::ResponseImageGenerationToolType::ImageGeneration,
        action: Some(ot::ResponseImageGenerationAction::Edit),
        background: response_image_background_from_request(body.background.clone()),
        input_fidelity: body.input_fidelity.as_ref().map(|f| match f {
            iet::OpenAiImageEditInputFidelity::High => {
                ot::ResponseImageGenerationInputFidelity::High
            }
            iet::OpenAiImageEditInputFidelity::Low => ot::ResponseImageGenerationInputFidelity::Low,
        }),
        input_image_mask: body
            .mask
            .as_ref()
            .map(|m| ot::ResponseImageGenerationInputImageMask {
                file_id: m.file_id.clone(),
                image_url: m.image_url.clone(),
            }),
        model: body
            .model
            .as_ref()
            .map(|m| response_image_model_from_create_image_edit_model(m.clone())),
        moderation: response_image_moderation_from_request(body.moderation.clone()),
        output_compression: body.output_compression.map(|c| c as u64),
        output_format: response_image_output_format_from_request(body.output_format.clone()),
        partial_images: body.partial_images,
        quality: response_image_quality_from_create_image_edit_request(body.quality.clone()),
        size: response_image_size_from_create_image_edit_request(body.size.clone()),
    };

    let model = body
        .model
        .as_ref()
        .map(create_image_edit_model_to_string)
        .unwrap_or_else(|| "gpt-image-1".to_string());

    Ok(OpenAiCreateResponseRequest {
        body: ResponseRequestBody {
            model: Some(model),
            input: Some(input),
            tools: Some(vec![
                crate::openai::create_response::types::ResponseTool::ImageGeneration(image_tool),
            ]),
            tool_choice: Some(image_tool_choice()),
            stream: body.stream,
            user: body.user.clone(),
            ..ResponseRequestBody::default()
        },
        ..OpenAiCreateResponseRequest::default()
    })
}
