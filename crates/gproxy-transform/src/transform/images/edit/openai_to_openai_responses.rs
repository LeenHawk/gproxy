//! OpenAI edit-image <-> OpenAI Responses transforms.
//!
//! Codex-like backends expose image editing through the Responses
//! `image_generation` tool with `action: "edit"`, not a dedicated images
//! endpoint. Keep this separate from create-image so edit inputs are not dropped.

use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::ImageEditRequest,
    _: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    if input.images.is_empty() {
        return Err(TransformError::InvalidInput {
            reason: "OpenAI edit-image request must contain at least one image".to_owned(),
        });
    }

    let mut parts = vec![openai::ResponseInputContentPart::InputText {
        text: format!("Edit the provided image(s) according to: {}", input.prompt),
        extra: Default::default(),
    }];
    parts.extend(input.images.into_iter().map(image_reference_to_input_image));

    let tool = openai::ResponseTool::ImageGeneration {
        action: Some(openai::ImageGenerationAction::Edit),
        background: input.background,
        input_fidelity: input.input_fidelity,
        input_image_mask: input.mask.map(image_reference_to_mask),
        model: None,
        moderation: input.moderation,
        output_compression: input.output_compression,
        output_format: input.output_format,
        partial_images: input.partial_images,
        quality: input.quality,
        size: edit_size_to_response_size(input.size),
        extra: Default::default(),
    };

    Ok(openai::ResponseCreateRequest {
        input: Some(openai::ResponseInput::Items(vec![
            openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
                openai::ResponseEasyInputMessageItem {
                    type_: Some(openai::ResponseMessageItemType::Message),
                    role: openai::ResponseEasyInputMessageRole::User,
                    content: openai::ResponseEasyInputContent::Parts(parts),
                    phase: None,
                    extra: Default::default(),
                },
            )),
        ])),
        model: input.model,
        stream: input.stream,
        tools: Some(vec![tool]),
        user: input.user,
        ..Default::default()
    })
}

pub fn response(
    input: openai::ResponseObject,
    ctx: &TransformContext,
) -> Result<openai::ImagesResponse, TransformError> {
    super::super::create::openai_to_openai_responses::response(input, ctx)
}

fn image_reference_to_input_image(
    reference: openai::ImageReference,
) -> openai::ResponseInputContentPart {
    openai::ResponseInputContentPart::InputImage {
        detail: None,
        file_id: reference.file_id,
        image_url: reference.image_url,
        extra: reference.extra,
    }
}

fn image_reference_to_mask(reference: openai::ImageReference) -> openai::ImageMask {
    openai::ImageMask {
        file_id: reference.file_id,
        image_url: reference.image_url,
        extra: reference.extra,
    }
}

fn edit_size_to_response_size(
    size: Option<openai::ImageEditSize>,
) -> Option<openai::ResponseImageGenerationSize> {
    let known = match size? {
        openai::ImageEditSize::Auto => openai::ResponseImageGenerationSizeKnown::Auto,
        openai::ImageEditSize::Size1024By1024 => {
            openai::ResponseImageGenerationSizeKnown::Size1024By1024
        }
        openai::ImageEditSize::Size1024By1536 => {
            openai::ResponseImageGenerationSizeKnown::Size1024By1536
        }
        openai::ImageEditSize::Size1536By1024 => {
            openai::ResponseImageGenerationSizeKnown::Size1536By1024
        }
    };
    Some(openai::ResponseImageGenerationSize::Known(known))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey, Provider};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::provider(Operation::EditImage, Provider::OpenAi),
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        )
    }

    #[test]
    fn edit_request_injects_image_generation_edit_tool_and_inputs() {
        let img: openai::ImageEditRequest = serde_json::from_value(json!({
            "prompt": "make it blue",
            "model": "gpt-5.4",
            "images": [
                { "image_url": "data:image/png;base64,AAAA" },
                { "file_id": "file_1" }
            ],
            "mask": { "image_url": "data:image/png;base64,BBBB" },
            "input_fidelity": "high",
            "output_compression": 80,
            "output_format": "png",
            "partial_images": 2,
            "quality": "high",
            "size": "1024x1536",
            "stream": false,
            "user": "user_1"
        }))
        .unwrap();
        let v = serde_json::to_value(request(img, &ctx()).unwrap()).unwrap();

        assert_eq!(v["model"], "gpt-5.4");
        assert_eq!(v["stream"], false);
        assert_eq!(v["user"], "user_1");
        assert_eq!(v["tools"][0]["type"], "image_generation");
        assert_eq!(v["tools"][0]["action"], "edit");
        assert_eq!(v["tools"][0]["input_fidelity"], "high");
        assert_eq!(
            v["tools"][0]["input_image_mask"]["image_url"],
            "data:image/png;base64,BBBB"
        );
        assert_eq!(v["tools"][0]["output_compression"], 80);
        assert_eq!(v["tools"][0]["output_format"], "png");
        assert_eq!(v["tools"][0]["partial_images"], 2);
        assert_eq!(v["tools"][0]["quality"], "high");
        assert_eq!(v["tools"][0]["size"], "1024x1536");

        let content = &v["input"][0]["content"];
        assert_eq!(content[0]["type"], "input_text");
        assert!(
            content[0]["text"]
                .as_str()
                .unwrap()
                .contains("make it blue")
        );
        assert_eq!(content[1]["type"], "input_image");
        assert_eq!(content[1]["image_url"], "data:image/png;base64,AAAA");
        assert_eq!(content[2]["type"], "input_image");
        assert_eq!(content[2]["file_id"], "file_1");
    }
}
