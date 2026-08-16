//! Create-video request/response conversions. The response mapping also
//! serves `RetrieveVideo` (both wire shapes are the same "job status" object).

use super::common;
use crate::protocol::{gemini, openai};
use crate::transform::context::report_unsupported;
use crate::transform::{TransformContext, TransformError};

pub mod openai_to_gemini {
    use super::*;

    pub fn request(
        input: openai::CreateVideoRequest,
        _: &TransformContext,
    ) -> Result<gemini::VeoGenerateVideosRequest, TransformError> {
        let (aspect_ratio, resolution) = input
            .size
            .as_ref()
            .map(common::size_to_veo)
            .unwrap_or((None, None));
        let duration_seconds = input.seconds.as_ref().and_then(|seconds| {
            serde_json::to_value(seconds)
                .ok()
                .and_then(|value| value.as_str().and_then(|value| value.parse().ok()))
        });
        let image = input.input_reference.and_then(|reference| match reference {
            openai::VideoInputReference::File(data_url) => match parse_data_url(&data_url) {
                Some((mime_type, data)) => Some(crate::protocol::wire!(gemini::VeoMedia {
                    bytes_base64_encoded: Some(data),
                    gcs_uri: None,
                    mime_type: Some(mime_type),
                    extra: Default::default(),
                })),
                None => {
                    report_unsupported(
                        "input_reference",
                        "Veo requires inline image bytes; the reference is not a data URL",
                    );
                    None
                }
            },
            _ => {
                report_unsupported(
                    "input_reference",
                    "Veo cannot resolve OpenAI file ids or external image URLs",
                );
                None
            }
        });
        Ok(crate::protocol::wire!(gemini::VeoGenerateVideosRequest {
            instances: vec![crate::protocol::wire!(gemini::VeoInstance {
                prompt: Some(input.prompt),
                image,
                extra: Default::default(),
            })],
            parameters: Some(crate::protocol::wire!(gemini::VeoParameters {
                aspect_ratio,
                duration_seconds,
                negative_prompt: None,
                number_of_videos: None,
                person_generation: None,
                resolution,
                seed: None,
                extra: Default::default(),
            })),
            extra: Default::default(),
        }))
    }

    pub fn response(
        input: openai::Video,
        _: &TransformContext,
    ) -> Result<gemini::VeoOperation, TransformError> {
        Ok(common::video_to_operation(input))
    }
}

pub mod gemini_to_openai {
    use super::*;

    pub fn request(
        input: gemini::VeoGenerateVideosRequest,
        _: &TransformContext,
    ) -> Result<openai::CreateVideoRequest, TransformError> {
        let instance = input.instances.into_iter().next();
        let prompt = instance
            .as_ref()
            .and_then(|instance| instance.prompt.clone())
            .ok_or_else(|| TransformError::InvalidInput {
                reason: "Veo request has no prompt instance".to_owned(),
            })?;
        if instance.as_ref().is_some_and(|value| value.image.is_some()) {
            report_unsupported(
                "instances[].image",
                "OpenAI create-video multipart references are not synthesized from inline Veo bytes",
            );
        }
        let parameters = input.parameters.unwrap_or_default();
        if parameters.negative_prompt.is_some() {
            report_unsupported(
                "parameters.negativePrompt",
                "OpenAI create-video has no negative prompt",
            );
        }
        let seconds = parameters.duration_seconds.map(|seconds| {
            serde_json::from_value(serde_json::json!(seconds.to_string()))
                .unwrap_or(openai::VideoSeconds::Unknown(seconds.to_string()))
        });
        Ok(crate::protocol::wire!(openai::CreateVideoRequest {
            prompt,
            input_reference: None,
            model: None,
            seconds,
            size: Some(common::veo_to_size(
                parameters.aspect_ratio.as_deref(),
                parameters.resolution.as_deref(),
            )),
            extra: Default::default(),
        }))
    }

    pub fn response(
        input: gemini::VeoOperation,
        _: &TransformContext,
    ) -> Result<openai::Video, TransformError> {
        Ok(common::operation_to_video(input))
    }
}

fn parse_data_url(value: &str) -> Option<(String, String)> {
    let rest = value.strip_prefix("data:")?;
    let (mime_type, data) = rest.split_once(";base64,")?;
    Some((mime_type.to_owned(), data.to_owned()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{Operation, OperationKey, Provider};
    use crate::transform::TransformContext;

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::provider(Operation::CreateVideo, Provider::OpenAi),
            OperationKey::provider(Operation::CreateVideo, Provider::Gemini),
        )
    }

    /// 尺寸→宽高比/分辨率与长时操作→任务状态是仅有的非平凡映射。
    #[test]
    fn maps_create_request_and_operation_status() {
        let request: openai::CreateVideoRequest = serde_json::from_value(json!({
            "prompt": "远景水墨山水",
            "model": "sora-2",
            "seconds": "8",
            "size": "1024x1792",
            "input_reference": "data:image/png;base64,AAAA"
        }))
        .unwrap();
        let veo = openai_to_gemini::request(request, &ctx()).unwrap();
        let value = serde_json::to_value(&veo).unwrap();
        assert_eq!(value["instances"][0]["prompt"], "远景水墨山水");
        assert_eq!(value["instances"][0]["image"]["bytesBase64Encoded"], "AAAA");
        assert_eq!(value["parameters"]["aspectRatio"], "9:16");
        assert_eq!(value["parameters"]["durationSeconds"], 8);

        let operation: gemini::VeoOperation = serde_json::from_value(json!({
            "name": "models/veo-3.1/operations/op1",
            "done": true,
            "response": {"generateVideoResponse": {"generatedSamples": [
                {"video": {"uri": "https://generativelanguage.googleapis.com/v1beta/files/f1:download?alt=media"}}
            ]}}
        }))
        .unwrap();
        let video = gemini_to_openai::response(operation, &ctx()).unwrap();
        assert_eq!(video.id, "models/veo-3.1/operations/op1");
        assert!(matches!(
            video.status,
            openai::VideoStatus::Known(openai::VideoStatusKnown::Completed)
        ));
        assert_eq!(video.progress, 100);
        assert!(
            video.extra["gproxy_video_uri"]
                .as_str()
                .unwrap()
                .contains("/files/f1:download")
        );
    }
}
