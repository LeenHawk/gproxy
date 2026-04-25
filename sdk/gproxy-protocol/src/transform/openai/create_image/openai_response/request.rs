use crate::openai::count_tokens::types as ot;
use crate::openai::create_image::request::{
    OpenAiCreateImageRequest, RequestBody as CreateImageRequestBody,
};
use crate::openai::create_image::types as it;
use crate::openai::create_response::request::{
    OpenAiCreateResponseRequest, RequestBody as ResponseRequestBody,
};
use crate::transform::openai::create_image::utils::{
    create_image_model_to_string, image_tool_choice, response_image_background_from_request,
    response_image_model_from_create_image_model, response_image_moderation_from_request,
    response_image_output_format_from_request, response_image_quality_from_create_image_request,
    response_image_size_from_create_image_request, user_message_from_parts,
};
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiCreateImageRequest> for OpenAiCreateResponseRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiCreateImageRequest) -> Result<Self, TransformError> {
        OpenAiCreateResponseRequest::try_from(&value)
    }
}

impl TryFrom<&OpenAiCreateImageRequest> for OpenAiCreateResponseRequest {
    type Error = TransformError;

    fn try_from(value: &OpenAiCreateImageRequest) -> Result<Self, TransformError> {
        create_image_to_response_request(&value.body)
    }
}

fn create_image_to_response_request(
    body: &CreateImageRequestBody,
) -> Result<OpenAiCreateResponseRequest, TransformError> {
    // Validate unsupported fields
    if matches!(
        body.response_format,
        Some(it::OpenAiImageResponseFormat::Url)
    ) {
        return Err(TransformError::not_implemented(
            "cannot convert OpenAI image request with response_format=url to responses.create request",
        ));
    }

    // Build the image_generation tool with all config from the create image request
    let image_tool = ot::ResponseImageGenerationTool {
        type_: ot::ResponseImageGenerationToolType::ImageGeneration,
        action: Some(ot::ResponseImageGenerationAction::Generate),
        background: response_image_background_from_request(body.background.clone()),
        input_fidelity: None,
        input_image_mask: None,
        model: body
            .model
            .as_ref()
            .map(|m| response_image_model_from_create_image_model(m.clone())),
        moderation: response_image_moderation_from_request(body.moderation.clone()),
        output_compression: body.output_compression.map(|c| c as u64),
        output_format: response_image_output_format_from_request(body.output_format.clone()),
        partial_images: body.partial_images,
        quality: response_image_quality_from_create_image_request(body.quality.clone()),
        size: response_image_size_from_create_image_request(body.size.clone())?,
    };

    // Build input: user message with text content
    let input = user_message_from_parts(vec![ot::ResponseInputContent::Text(
        ot::ResponseInputText {
            text: body.prompt.clone(),
            type_: ot::ResponseInputTextType::InputText,
        },
    )]);

    // Determine model name for the response request
    let model = body
        .model
        .as_ref()
        .map(create_image_model_to_string)
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
