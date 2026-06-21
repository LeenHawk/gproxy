//! OpenAI create-image <-> OpenAI Responses transforms.
//!
//! Codex/ChatGPT has no dedicated `/v1/images/generations` endpoint; it
//! generates images through the Responses API `image_generation` tool. So an
//! images request is reshaped into a Responses request that invokes that tool,
//! and the resulting `image_generation_call` output item (a base64 image) is
//! reshaped back into an images response.

use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::ImageGenerationRequest,
    _: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    // Only `output_format` carries over cleanly into the tool config; size /
    // quality / background use Responses-specific enums and are left to the
    // backend default (the prompt still drives the generation).
    let tool = openai::ResponseTool::ImageGeneration {
        action: None,
        background: None,
        input_fidelity: None,
        input_image_mask: None,
        model: None,
        moderation: None,
        output_compression: input.output_compression,
        output_format: input.output_format,
        partial_images: None,
        quality: None,
        size: None,
        extra: Default::default(),
    };
    Ok(openai::ResponseCreateRequest {
        model: input.model,
        // The images endpoint implies "generate an image"; as a Responses
        // message the bare description may not trigger the tool, so make the
        // intent explicit.
        input: Some(openai::ResponseInput::Text(format!(
            "Generate an image for the following prompt: {}",
            input.prompt
        ))),
        tools: Some(vec![tool]),
        ..Default::default()
    })
}

pub fn response(
    input: openai::ResponseObject,
    _: &TransformContext,
) -> Result<openai::ImagesResponse, TransformError> {
    let data: Vec<openai::Image> = input
        .output
        .into_iter()
        .filter_map(|item| match item.0 {
            openai::ResponseItem::Typed(openai::TypedResponseItem::ImageGenerationCall {
                result,
                ..
            }) => Some(openai::Image {
                b64_json: Some(result),
                revised_prompt: None,
                url: None,
                extra: Default::default(),
            }),
            _ => None,
        })
        .collect();
    Ok(openai::ImagesResponse {
        created: input.created_at,
        background: None,
        data: Some(data),
        output_format: None,
        quality: None,
        size: None,
        usage: None,
        extra: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey, Provider};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::provider(Operation::CreateImage, Provider::OpenAi),
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        )
    }

    #[test]
    fn images_request_injects_image_generation_tool() {
        let img: openai::ImageGenerationRequest =
            serde_json::from_str(r#"{"prompt":"a red cube","model":"gpt-5.4"}"#).unwrap();
        let v = serde_json::to_value(request(img, &ctx()).unwrap()).unwrap();
        assert_eq!(v["tools"][0]["type"], "image_generation");
        assert!(v["input"].as_str().unwrap().contains("a red cube"));
        assert_eq!(v["model"], "gpt-5.4");
    }

    #[test]
    fn responses_image_call_becomes_b64_image() {
        let obj: openai::ResponseObject = serde_json::from_str(
            r#"{"id":"resp_1","created_at":7,"object":"response","output":[{"type":"image_generation_call","id":"ig_1","result":"QkFTRTY0","status":"completed"}]}"#,
        )
        .unwrap();
        let data = response(obj, &ctx()).unwrap().data.unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].b64_json.as_deref(), Some("QkFTRTY0"));
    }
}
